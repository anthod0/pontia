use axum::{
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use ring::{digest, hmac};
use uuid::Uuid;

use crate::{Edge, browser_http::bearer_token, credential::parse_credential};

const KEY_LABEL: &[u8] = b"pontia-edge-ownership-key-v1\0";
const PROOF_LABEL: &[u8] = b"pontia-edge-ownership-v1\0";

/// Binds a Cloud challenge to this edge's service credential and public endpoint.
fn proof(edge_id: Uuid, secret: &[u8; 32], hostname: &str, port: u16, nonce: &str) -> String {
    let key = digest::digest(&digest::SHA256, &[KEY_LABEL, secret].concat());
    let key = hmac::Key::new(hmac::HMAC_SHA256, key.as_ref());
    let message = [
        PROOF_LABEL,
        edge_id.to_string().as_bytes(),
        b"\0",
        hostname.as_bytes(),
        b"\0",
        port.to_string().as_bytes(),
        b"\0",
        nonce.as_bytes(),
    ]
    .concat();
    URL_SAFE_NO_PAD.encode(hmac::sign(&key, &message))
}

pub(crate) async fn verify(edge: Edge, headers: HeaderMap, hostname: &str, port: u16) -> Response {
    let Some(nonce) = bearer_token(&headers)
        .and_then(|token| edge.access_tokens.ownership_nonce(token, hostname, port))
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some((edge_id, secret)) = parse_credential(edge.redeemer.service_credential()) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    (
        [
            (header::CONTENT_TYPE, "text/plain".to_owned()),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (
                header::HeaderName::from_static("pontia-edge-version"),
                pontia_version::version().to_string(),
            ),
        ],
        proof(edge_id, &secret, hostname, port, &nonce),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_matches_the_cloud_test_vector() {
        // apps/cloud/tests/edge-ownership.test.ts verifies the same vector.
        assert_eq!(
            proof(
                Uuid::parse_str("0199791c-6600-7000-8000-000000000001").unwrap(),
                &[42; 32],
                "edge.example.com",
                443,
                &"A".repeat(43),
            ),
            "G8PT0z2_FUc1KfPw8pFPg35CeSoVicv3je5l1O_UAyo"
        );
    }

    #[test]
    fn proof_depends_on_every_bound_value() {
        let edge_id = Uuid::parse_str("0199791c-6600-7000-8000-000000000001").unwrap();
        let nonce = "A".repeat(43);
        let expected = proof(edge_id, &[42; 32], "edge.example.com", 443, &nonce);
        for other in [
            proof(Uuid::now_v7(), &[42; 32], "edge.example.com", 443, &nonce),
            proof(edge_id, &[43; 32], "edge.example.com", 443, &nonce),
            proof(edge_id, &[42; 32], "other.example.com", 443, &nonce),
            proof(edge_id, &[42; 32], "edge.example.com", 8443, &nonce),
            proof(edge_id, &[42; 32], "edge.example.com", 443, &"B".repeat(43)),
        ] {
            assert_ne!(other, expected);
        }
    }
}

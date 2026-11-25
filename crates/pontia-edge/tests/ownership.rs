mod support;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::get_current_timestamp;
use reqwest::StatusCode;
use serde_json::json;
use support::{EDGE_ID, TestEdge, signed_jwt};
use uuid::Uuid;

const TYP: &str = "pontia-edge-ownership+jwt";

#[tokio::test]
async fn only_a_fresh_cloud_challenge_for_this_endpoint_receives_a_proof() {
    let server = TestEdge::start().await;
    let port: u16 = server
        .edge_origin
        .rsplit(':')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let verify = || {
        server
            .http
            .get(format!("{}/__pontia/verify-ownership", server.edge_origin))
    };
    let now = get_current_timestamp();
    let nonce = URL_SAFE_NO_PAD.encode([9; 32]);
    let claims = |audience: &str, host: &str, port: u16, nonce: &str, iat: u64, exp: u64| json!({"aud": audience, "jti": nonce, "host": host, "port": port, "iat": iat, "exp": exp});
    let valid = claims(EDGE_ID, "127.0.0.1", port, &nonce, now, now + 60);

    assert_eq!(
        verify().send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let other_edge = Uuid::now_v7().to_string();
    for token in [
        signed_jwt("JWT", &valid, &[5; 32]),
        signed_jwt("pontia-edge-update+jwt", &valid, &[5; 32]),
        signed_jwt(TYP, &valid, &[6; 32]),
        signed_jwt(
            TYP,
            &claims(&other_edge, "127.0.0.1", port, &nonce, now, now + 60),
            &[5; 32],
        ),
        signed_jwt(
            TYP,
            &claims(EDGE_ID, "other.example.com", port, &nonce, now, now + 60),
            &[5; 32],
        ),
        signed_jwt(
            TYP,
            &claims(EDGE_ID, "127.0.0.1", 443, &nonce, now, now + 60),
            &[5; 32],
        ),
        signed_jwt(
            TYP,
            &claims(EDGE_ID, "127.0.0.1", port, &nonce, now - 61, now - 1),
            &[5; 32],
        ),
        signed_jwt(
            TYP,
            &claims(EDGE_ID, "127.0.0.1", port, &nonce, now, now + 61),
            &[5; 32],
        ),
        signed_jwt(
            TYP,
            &claims(EDGE_ID, "127.0.0.1", port, &"A".repeat(42), now, now + 60),
            &[5; 32],
        ),
        signed_jwt(
            TYP,
            &claims(
                EDGE_ID,
                "127.0.0.1",
                port,
                &format!("{nonce}="),
                now,
                now + 60,
            ),
            &[5; 32],
        ),
    ] {
        let response = verify().bearer_auth(token).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert!(response.text().await.unwrap().is_empty());
    }

    let response = verify()
        .bearer_auth(signed_jwt(TYP, &valid, &[5; 32]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "text/plain");
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert!(response.headers().contains_key("pontia-edge-version"));
    let proof = response.text().await.unwrap();
    assert_eq!(proof.len(), 43);
    assert_eq!(URL_SAFE_NO_PAD.decode(&proof).unwrap().len(), 32);
}

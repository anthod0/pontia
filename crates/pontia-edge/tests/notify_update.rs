mod support;

use jsonwebtoken::{Algorithm, EncodingKey, Header, encode, get_current_timestamp};
use reqwest::StatusCode;
use serde_json::json;
use support::{EDGE_ID, TestEdge, signed_jwt};
use uuid::Uuid;

#[tokio::test]
async fn only_a_fresh_cloud_update_for_this_edge_consumes_the_health_nonce() {
    let server = TestEdge::start().await;
    let version = || {
        server
            .http
            .post(format!("{}/__pontia/version", server.edge_origin))
    };
    assert_eq!(
        version().send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let now = get_current_timestamp();
    let version_claims = json!({"aud":EDGE_ID,"iat":now,"exp":now+60});
    let version_typ = "pontia-edge-version+jwt";
    let mut hmac_header = Header::new(Algorithm::HS256);
    hmac_header.typ = Some(version_typ.to_owned());
    for token in [
        encode(
            &hmac_header,
            &version_claims,
            &EncodingKey::from_secret(b"not-an-ed25519-key"),
        )
        .unwrap(),
        signed_jwt("JWT", &version_claims, &[5; 32]),
        signed_jwt("pontia-edge-update+jwt", &version_claims, &[5; 32]),
        signed_jwt(version_typ, &version_claims, &[6; 32]),
        signed_jwt(
            version_typ,
            &json!({"aud":Uuid::now_v7(),"iat":now,"exp":now+60}),
            &[5; 32],
        ),
        signed_jwt(
            version_typ,
            &json!({"aud":EDGE_ID,"iat":now-61,"exp":now-1}),
            &[5; 32],
        ),
        signed_jwt(
            version_typ,
            &json!({"aud":EDGE_ID,"iat":now,"exp":now+61}),
            &[5; 32],
        ),
    ] {
        assert_eq!(
            version().bearer_auth(token).send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let response = version()
        .bearer_auth(signed_jwt(version_typ, &version_claims, &[5; 32]))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()["pontia-edge-version"],
        include_str!("../../../VERSION").trim()
    );
    assert_eq!(response.headers()["cache-control"], "no-store");
    let nonce = response.headers()["pontia-update-nonce"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(response.text().await.unwrap(), "ok");
    let notify = || {
        server
            .http
            .post(format!("{}/__pontia/notify-update", server.edge_origin))
    };
    assert_eq!(
        notify().send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let claims = json!({"aud":EDGE_ID,"iat":now,"exp":now+60,"jti":nonce});
    let typ = "pontia-edge-update+jwt";
    for token in [
        signed_jwt("JWT", &claims, &[5; 32]),
        signed_jwt("pontia-preview+jwt", &claims, &[5; 32]),
        signed_jwt(typ, &claims, &[6; 32]),
        signed_jwt(
            typ,
            &json!({"aud":Uuid::now_v7(),"iat":now,"exp":now+60,"jti":nonce}),
            &[5; 32],
        ),
        signed_jwt(
            typ,
            &json!({"aud":EDGE_ID,"iat":now-61,"exp":now-1,"jti":nonce}),
            &[5; 32],
        ),
        signed_jwt(
            typ,
            &json!({"aud":EDGE_ID,"iat":now,"exp":now+3600,"jti":nonce}),
            &[5; 32],
        ),
    ] {
        assert_eq!(
            notify().bearer_auth(token).send().await.unwrap().status(),
            StatusCode::UNAUTHORIZED
        );
    }
    let token = signed_jwt(typ, &claims, &[5; 32]);
    let send = || notify().bearer_auth(&token).send();
    let (first, second) = tokio::join!(send(), send());
    let mut statuses = [first.unwrap().status(), second.unwrap().status()];
    statuses.sort();
    assert!(matches!(
        statuses[0],
        StatusCode::ACCEPTED | StatusCode::NO_CONTENT
    ));
    assert_eq!(statuses[1], StatusCode::CONFLICT);
}

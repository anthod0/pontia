use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use pontia_http::HttpEntrypoints;
use tower::ServiceExt;

const TOKEN: &str = "test-token";

fn request(path: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder().uri(path);
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    builder.body(Body::empty()).expect("request")
}

#[tokio::test]
async fn local_http_entrypoint_requires_the_configured_bearer_token() {
    let state = TestApp::builder()
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await;
    let entrypoints = HttpEntrypoints::new(state);

    let valid = entrypoints
        .local_http()
        .oneshot(request("/api/v1/auth/validate", Some(TOKEN)))
        .await
        .expect("response");
    let missing = entrypoints
        .local_http()
        .oneshot(request("/api/v1/auth/validate", None))
        .await
        .expect("response");
    let wrong = entrypoints
        .local_http()
        .oneshot(request("/api/v1/auth/validate", Some("wrong-token")))
        .await
        .expect("response");

    assert_eq!(valid.status(), StatusCode::OK);
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(wrong.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn local_http_headers_cannot_claim_trusted_tunnel_access() {
    let state = TestApp::builder()
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await;
    let entrypoints = HttpEntrypoints::new(state);
    let request = Request::builder()
        .uri("/api/v1/auth/validate")
        .header("x-pontia-ingress", "trusted-device-tunnel")
        .header("x-pontia-trusted-tunnel-request", "true")
        .body(Body::empty())
        .expect("request");

    let response = entrypoints
        .local_http()
        .oneshot(request)
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

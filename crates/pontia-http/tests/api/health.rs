use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use tower::ServiceExt;

use pontia_application::AppState;
use pontia_http as http;

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("health.db")
        .external_api_token(None)
        .build_state()
        .await
}

#[tokio::test]
async fn healthz_returns_ok_json() {
    let response = http::router(test_state().await)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);

    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).expect("json body");

    assert_eq!(json, serde_json::json!({ "status": "ok" }));
}

#[tokio::test]
async fn obsolete_agent_http_endpoints_are_removed() {
    let state = test_state().await;
    for (method, path) in [
        ("POST", "/internal/v1/events"),
        ("POST", "/internal/v1/sessions/session/turn-start-failure"),
        (
            "GET",
            "/internal/v1/agent-bindings?client_type=pi&client_session_key=native",
        ),
        (
            "GET",
            "/internal/v1/agent-bindings/current-turn?client_type=pi&client_session_key=native",
        ),
    ] {
        let response = http::router(state.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("content-type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}

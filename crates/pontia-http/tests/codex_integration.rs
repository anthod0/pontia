mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use common::test_app::TestApp;

async fn request(app: &TestApp, request: Request<Body>) -> (StatusCode, Value) {
    let response = pontia_http::router(app.state.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

#[tokio::test]
async fn codex_sessions_are_created_without_a_runtime() {
    let app = TestApp::new().await;
    assert!(app.state.clients().spec("codex").is_some());
    let (status, body) = request(
        &app,
        Request::post("/api/v1/sessions")
            .header("authorization", "Bearer test-token")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"client_type":"codex","workspace":app.workspace().path()}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert!(body["data"]["session"]["session_id"].is_string());
    assert_eq!(body["data"]["session"]["client_type"], "codex");
    assert_eq!(body["data"]["session"]["capabilities"]["accept_task"], true);
}

#[tokio::test]
async fn codex_tui_control_routes_address_a_session() {
    let app = TestApp::new().await;
    for action in ["start", "stop"] {
        let (status, _) = request(
            &app,
            Request::post(format!("/api/v1/sessions/missing/tui/{action}"))
                .header("authorization", "Bearer test-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}

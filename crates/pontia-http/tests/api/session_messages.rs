use crate::common::{generic_client::GenericClientTestScope, test_app::TestApp};
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> (AppState, tempfile::TempDir, tempfile::TempDir) {
    let (state, pontia_home) = TestApp::builder()
        .database_name("messages.db")
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await;
    let workspace = crate::common::test_app::registered_workspace(&state).await;
    (state, workspace, pontia_home)
}

async fn request(
    state: AppState,
    method: &str,
    uri: &str,
    idempotency_key: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"));
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if let Some(key) = idempotency_key {
        builder = builder.header("Idempotency-Key", key);
    }
    let response = http::router(state)
        .oneshot(
            builder
                .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("json body")
    };
    (status, body)
}

async fn create_session(state: AppState) -> String {
    let workspace_id = crate::common::test_app::workspace_id(&state).await;
    let (status, body) = request(
        state,
        "POST",
        "/api/v1/sessions",
        None,
        Some(json!({"client_type":"generic","workspace_id":workspace_id})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["session"]["session_id"]
        .as_str()
        .expect("session id")
        .to_owned()
}

async fn send(state: AppState, session_id: &str, key: &str, body: Value) -> (StatusCode, Value) {
    request(
        state,
        "POST",
        &format!("/api/v1/sessions/{session_id}/messages"),
        Some(key),
        Some(body),
    )
    .await
}

#[tokio::test]
async fn immediate_message_delivers_and_repeated_idempotency_key_returns_the_same_result() {
    let _scope = GenericClientTestScope::new().await;
    let (state, _workspace, _pontia_home) = test_state().await;
    let session_id = create_session(state.clone()).await;

    let first = send(
        state.clone(),
        &session_id,
        "same-message",
        json!({"input":"continue","mode":"immediate","metadata":{"source":"test"}}),
    )
    .await;
    let duplicate = send(
        state.clone(),
        &session_id,
        "same-message",
        json!({"input":"ignored","mode":"queue"}),
    )
    .await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(first.1["data"]["delivery"]["status"], "delivered");
    assert_eq!(duplicate.0, StatusCode::OK);
    assert_eq!(duplicate.1["data"], first.1["data"]);
    let turns: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turns WHERE session_id=?")
        .bind(&session_id)
        .fetch_one(&state.db())
        .await
        .unwrap();
    assert_eq!(turns, 1);
}

#[tokio::test]
async fn interrupt_mode_delivers_when_the_session_is_idle() {
    let _scope = GenericClientTestScope::new().await;
    let (state, _workspace, _pontia_home) = test_state().await;
    let session_id = create_session(state.clone()).await;

    let (status, body) = send(
        state,
        &session_id,
        "interrupt-idle",
        json!({"input":"continue","mode":"interrupt"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["delivery"]["status"], "delivered");
}

#[tokio::test]
async fn invalid_message_mode_uses_the_api_error_envelope() {
    let _scope = GenericClientTestScope::new().await;
    let (state, _workspace, _pontia_home) = test_state().await;
    let session_id = create_session(state.clone()).await;

    let (status, body) = send(
        state,
        &session_id,
        "invalid-mode",
        json!({"input":"continue","mode":"later"}),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn busy_message_without_steer_falls_back_to_queue_and_can_be_cancelled() {
    let _scope = GenericClientTestScope::new().await;
    let (state, _workspace, _pontia_home) = test_state().await;
    let session_id = create_session(state.clone()).await;
    let (status, _) = send(
        state.clone(),
        &session_id,
        "first",
        json!({"input":"first","mode":"immediate"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    sqlx::query("UPDATE sessions SET state='busy' WHERE session_id=?")
        .bind(&session_id)
        .execute(&state.db())
        .await
        .unwrap();

    let (status, body) = send(
        state.clone(),
        &session_id,
        "second",
        json!({"input":"second","mode":"immediate"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["data"]["delivery"]["status"], "queued");
    let message_id = body["data"]["delivery"]["message_id"].as_str().unwrap();

    let (status, body) = request(
        state.clone(),
        "GET",
        &format!("/api/v1/sessions/{session_id}/queue"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["messages"][0]["message_id"], message_id);
    assert_eq!(body["data"]["messages"][0]["input"], "second");
    assert!(body["data"]["messages"][0].get("state").is_none());

    let (status, _) = request(
        state.clone(),
        "DELETE",
        &format!("/api/v1/sessions/{session_id}/queue/{message_id}"),
        Some("cancel-second"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = request(
        state,
        "GET",
        &format!("/api/v1/sessions/{session_id}/queue"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"]["messages"], json!([]));
}

#[tokio::test]
async fn legacy_inbox_routes_are_removed() {
    let _scope = GenericClientTestScope::new().await;
    let (state, _workspace, _pontia_home) = test_state().await;
    let session_id = create_session(state.clone()).await;
    let (status, _) = request(
        state,
        "GET",
        &format!("/api/v1/sessions/{session_id}/inbox/messages"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

use crate::common::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

mod context;
mod lifecycle;
mod tmux;
mod upsert;

pub(super) async fn test_state() -> (AppState, TestApp) {
    let app = TestApp::builder()
        .database_name("runtime-binding-upsert.db")
        .external_api_token(Some("test-token".to_string()))
        .build()
        .await;
    (app.state.clone(), app)
}

pub(super) async fn post_upsert(state: AppState, body: Value) -> (StatusCode, Value) {
    request_json(
        state,
        "POST",
        "/internal/v1/runtime-bindings/upsert",
        Some(body),
    )
    .await
}

pub(super) async fn get_current_turn_by_client_session(
    state: AppState,
    client_type: &str,
    client_session_key: &str,
) -> (StatusCode, Value) {
    request_json(
        state,
        "GET",
        &format!(
            "/internal/v1/agent-bindings/current-turn?client_type={client_type}&client_session_key={client_session_key}",
        ),
        None,
    )
    .await
}

pub(super) async fn get_session_context_by_client_session(
    state: AppState,
    client_type: &str,
    client_session_key: &str,
) -> (StatusCode, Value) {
    request_json(
        state,
        "GET",
        &format!(
            "/internal/v1/agent-bindings/session-context?client_type={client_type}&client_session_key={client_session_key}",
        ),
        None,
    )
    .await
}

pub(super) async fn delete_session(state: AppState, session_id: &str) -> (StatusCode, Value) {
    request_json(
        state,
        "DELETE",
        &format!("/external/v1/sessions/{session_id}"),
        None,
    )
    .await
}

pub(super) async fn request_json(
    state: AppState,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    if uri.starts_with("/external/v1/") {
        builder = builder.header(header::AUTHORIZATION, "Bearer test-token");
    }
    let response = http::router(state)
        .oneshot(
            builder
                .body(Body::from(
                    body.map(|body| body.to_string()).unwrap_or_default(),
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    let status = response.status();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let json = serde_json::from_slice(&body).expect("json body");
    (status, json)
}

pub(super) fn upsert_body(workspace: &str, pane_id: Option<&str>) -> Value {
    upsert_body_with_tmux(workspace, "/tmp/tmux-1000/default", pane_id, Some("dev"))
}

pub(super) fn upsert_body_with_tmux(
    workspace: &str,
    socket_path: &str,
    pane_id: Option<&str>,
    session_name: Option<&str>,
) -> Value {
    let tmux = pane_id.map(|pane_id| {
        json!({
            "socket_path": socket_path,
            "session_id": "$1",
            "session_name": session_name,
            "window_id": "@3",
            "window_index": 0,
            "pane_id": pane_id,
            "pane_index": 1,
            "pane_current_path": workspace
        })
    });
    json!({
        "client_type": "pi",
        "client_session_key": "pi_session_123",
        "client_session_file": "/tmp/pi/session.jsonl",
        "client_session_dir": "/tmp/pi",
        "client_cwd": workspace,
        "launch_cwd": workspace,
        "start_command": "pi --approve",
        "tmux": tmux
    })
}

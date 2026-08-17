use std::time::Duration;

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

async fn insert_approval_context(state: &AppState, runtime_metadata: &str) {
    let internal_event_url = serde_json::from_str::<Value>(runtime_metadata)
        .expect("runtime context json")
        .get("internal_event_url")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES ('sess_approval', 'claude', 'busy', 'turn_approval', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert session");
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, input_summary, metadata)
           VALUES ('turn_approval', 'sess_approval', 'running', 'work', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert turn");
    sqlx::query(
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, internal_event_url)
           VALUES ('sess_approval', 'claude_tui', 'rtinst_approval', ?)"#,
    )
    .bind(internal_event_url)
    .execute(&state.db())
    .await
    .expect("insert runtime binding");
    sqlx::query(
        r#"INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES ('binding_approval', 'sess_approval', 'claude', '/repo', 'claude_native', '{}')"#,
    )
    .execute(&state.db())
    .await
    .expect("insert agent binding");
}

async fn post(state: AppState, uri: &'static str, body: Value) -> axum::response::Response {
    http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn post_external(
    state: AppState,
    uri: &str,
    body: Value,
    idempotency_key: &str,
) -> axum::response::Response {
    http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header(header::AUTHORIZATION, "Bearer test-token")
                .header("Idempotency-Key", idempotency_key)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response")
}

async fn configured_otel_state(database_name: &str) -> AppState {
    let state = TestApp::builder()
        .database_name(database_name)
        .external_api_token(Some("test-token".to_string()))
        .build_state()
        .await;
    insert_approval_context(&state, "{}").await;
    state
}

async fn approval_event_id(state: &AppState) -> String {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(event_id) = sqlx::query_scalar::<_, String>(
                "SELECT event_id FROM events WHERE event_type = 'approval.requested'",
            )
            .fetch_optional(&state.db())
            .await
            .expect("approval event")
            {
                break event_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("approval event timeout")
}

fn permission_request_body() -> Value {
    json!({
        "session_id": "claude_native",
        "prompt_id": "prompt_1",
        "tool_name": "Bash",
        "tool_input": {"command": "pnpm test"},
        "permission_suggestions": [{
            "type": "addRules",
            "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
            "behavior": "allow",
            "destination": "localSettings"
        }],
        "hook_input": {
            "hook_event_name": "PermissionRequest",
            "session_id": "claude_native",
            "prompt_id": "prompt_1",
            "tool_name": "Bash",
            "tool_input": {"command": "pnpm test"}
        }
    })
}

mod commands;
mod otlp;
mod permission_request;

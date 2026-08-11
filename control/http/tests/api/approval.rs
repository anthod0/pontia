use std::time::Duration;

use crate::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::AppState;
use pontia_http as http;
use serde_json::{Value, json};
use tower::ServiceExt;

async fn configured_state() -> AppState {
    let state = TestApp::builder()
        .database_name("approval-registration.db")
        .build_state()
        .await;
    insert_approval_context(
        &state,
        r#"{"internal_event_url":"http://127.0.0.1/internal/v1/events"}"#,
    )
    .await;
    state
}

async fn insert_approval_context(state: &AppState, runtime_metadata: &str) {
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
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata)
           VALUES ('sess_approval', 'claude_tui', 'rtinst_approval', ?)"#,
    )
    .bind(runtime_metadata)
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

async fn post_otlp(
    state: AppState,
    body: Value,
    authorization: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method("POST")
        .uri("/internal/v1/otel/v1/logs")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(authorization) = authorization {
        request = request.header(header::AUTHORIZATION, authorization);
    }
    http::router(state)
        .oneshot(request.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response")
}

fn tool_decision_fixture() -> Value {
    serde_json::from_str(include_str!("../fixtures/otlp/claude-tool-decision.json"))
        .expect("valid OTLP JSON fixture")
}

fn set_tool_decision_attribute(fixture: &mut Value, key: &str, value: &str) {
    let attributes = fixture["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["attributes"]
        .as_array_mut()
        .expect("fixture log attributes");
    let attribute = attributes
        .iter_mut()
        .find(|attribute| attribute["key"] == key)
        .expect("fixture attribute");
    attribute["value"]["stringValue"] = Value::String(value.to_string());
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

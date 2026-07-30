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
           VALUES ('sess_approval', 'claude_tui', 'rtinst_approval', '{"internal_event_url":"http://127.0.0.1/internal/v1/events"}')"#,
    )
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
    state
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

#[tokio::test]
async fn permission_request_projects_bounded_snapshot_and_waits_until_turn_terminal() {
    let state = configured_state().await;
    let request_state = state.clone();
    let request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            json!({
                "session_id": "claude_native",
                "prompt_id": "prompt_1",
                "tool_name": "Bash",
                "tool_input": {
                    "command": "secret command that must stay transient",
                    "description": "secret description"
                },
                "permission_suggestions": [
                    {
                        "type": "addRules",
                        "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                        "behavior": "allow",
                        "destination": "localSettings"
                    },
                    {
                        "type": "addRules",
                        "rules": [{"toolName": "Bash"}],
                        "behavior": "allow",
                        "destination": "localSettings",
                        "unknown": true
                    }
                ],
                "hook_input": {
                    "hook_event_name": "PermissionRequest",
                    "session_id": "claude_native",
                    "tool_name": "Bash",
                    "tool_input": {"command": "secret command that must stay transient"}
                },
                "unexpected_hook_payload": "must be rejected"
            }),
        )
        .await
    });

    // Unknown request fields fail closed before creating durable state.
    let response = request.await.expect("request task");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'approval.requested'")
            .fetch_one(&state.db())
            .await
            .expect("event count");
    assert_eq!(count, 0);

    let request_state = state.clone();
    let request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            json!({
                "session_id": "claude_native",
                "prompt_id": "prompt_1",
                "tool_name": "Bash",
                "tool_input": {"command": "secret command that must stay transient"},
                "permission_suggestions": [
                    {
                        "type": "addRules",
                        "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                        "behavior": "allow",
                        "destination": "localSettings"
                    },
                    {
                        "type": "addRules",
                        "rules": [{"toolName": "Bash"}],
                        "behavior": "allow",
                        "destination": "localSettings",
                        "unknown": true
                    }
                ],
                "hook_input": {
                    "hook_event_name": "PermissionRequest",
                    "session_id": "claude_native",
                    "prompt_id": "prompt_1",
                    "tool_name": "Bash",
                    "tool_input": {"command": "secret command that must stay transient"},
                    "permission_suggestions": [{"raw_invalid_suggestion": true}]
                }
            }),
        )
        .await
    });

    let event = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(row) = sqlx::query_as::<_, (String, String, String, String)>(
                r#"SELECT event_id, session_id, turn_id, payload
                   FROM events WHERE event_type = 'approval.requested'"#,
            )
            .fetch_optional(&state.db())
            .await
            .expect("approval event")
            {
                break row;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("approval event timeout");
    let payload: Value = serde_json::from_str(&event.3).expect("payload");
    assert_eq!(event.1, "sess_approval");
    assert_eq!(event.2, "turn_approval");
    assert_eq!(payload["client_session_id"], "claude_native");
    assert_eq!(payload["prompt_id"], "prompt_1");
    assert_eq!(payload["tool_name"], "Bash");
    assert_eq!(
        payload["permission_suggestions"].as_array().unwrap().len(),
        1
    );
    assert!(payload.get("tool_input").is_none());
    assert!(!event.3.contains("secret command"));

    let (state_value, metadata): (String, String) =
        sqlx::query_as("SELECT state, metadata FROM sessions WHERE session_id = 'sess_approval'")
            .fetch_one(&state.db())
            .await
            .expect("session projection");
    assert_eq!(state_value, "busy");
    let metadata: Value = serde_json::from_str(&metadata).expect("metadata");
    assert_eq!(metadata["interaction"]["type"], "approval");
    assert_eq!(metadata["interaction"]["state"], "awaiting");
    assert_eq!(metadata["interaction"]["request_event_id"], event.0);

    let terminal = post(
        state.clone(),
        "/internal/v1/events",
        json!({
            "session_id": "sess_approval",
            "turn_id": "turn_approval",
            "type": "turn.completed",
            "data": {"runtime_instance_id": "rtinst_approval"}
        }),
    )
    .await;
    assert_eq!(terminal.status(), StatusCode::OK);

    let response = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("waiter release timeout")
        .expect("request task");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body: Value = serde_json::from_slice(&body).expect("json");
    assert_eq!(body["data"]["result"], "resolved_elsewhere");

    let metadata: String =
        sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id = 'sess_approval'")
            .fetch_one(&state.db())
            .await
            .expect("session metadata");
    assert!(
        serde_json::from_str::<Value>(&metadata)
            .unwrap()
            .get("interaction")
            .is_none()
    );
    let final_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type LIKE 'approval.%' AND event_type != 'approval.requested'",
    )
    .fetch_one(&state.db())
    .await
    .expect("final count");
    assert_eq!(final_count, 0);

    let no_active_turn = post(
        state.clone(),
        "/internal/v1/claude/permission-request",
        json!({
            "session_id": "claude_native",
            "tool_name": "Bash",
            "tool_input": {},
            "permission_suggestions": [],
            "hook_input": {
                "hook_event_name": "PermissionRequest",
                "session_id": "claude_native",
                "tool_name": "Bash",
                "tool_input": {}
            }
        }),
    )
    .await;
    assert_eq!(no_active_turn.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        no_active_turn
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes()
            .len(),
        0
    );
    let request_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'approval.requested'")
            .fetch_one(&state.db())
            .await
            .expect("request count");
    assert_eq!(request_count, 1);

    let approval_tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND lower(name) LIKE '%approval%'",
    )
    .fetch_one(&state.db())
    .await
    .expect("approval table count");
    assert_eq!(approval_tables, 0);
}

#[tokio::test]
async fn unbound_permission_request_is_an_empty_success_and_creates_no_event() {
    let app = TestApp::new().await;
    let state = app.state.clone();
    let response = post(
        state.clone(),
        "/internal/v1/claude/permission-request",
        json!({
            "session_id": "missing",
            "tool_name": "Bash",
            "tool_input": {},
            "permission_suggestions": [],
            "hook_input": {
                "hook_event_name": "PermissionRequest",
                "session_id": "missing",
                "tool_name": "Bash",
                "tool_input": {}
            }
        }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes()
            .len(),
        0
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'approval.requested'")
            .fetch_one(&state.db())
            .await
            .expect("event count");
    assert_eq!(count, 0);
}

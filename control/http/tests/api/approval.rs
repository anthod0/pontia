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

#[tokio::test]
async fn otlp_tool_decision_authenticates_and_resolves_the_matching_approval_once() {
    let state = configured_otel_state("approval-otlp-once.db").await;

    let request_state = state.clone();
    let hook_request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    let request_event_id = approval_event_id(&state).await;
    let fixture = tool_decision_fixture();

    let missing_auth = post_otlp(state.clone(), fixture.clone(), None).await;
    assert_eq!(missing_auth.status(), StatusCode::UNAUTHORIZED);
    assert!(!hook_request.is_finished());
    let wrong_auth = post_otlp(state.clone(), fixture.clone(), Some("Bearer wrong-token")).await;
    assert_eq!(wrong_auth.status(), StatusCode::UNAUTHORIZED);
    assert!(!hook_request.is_finished());

    let response = post_otlp(state.clone(), fixture.clone(), Some("Bearer test-token")).await;
    assert_eq!(response.status(), StatusCode::OK);

    let hook_response = tokio::time::timeout(Duration::from_secs(2), hook_request)
        .await
        .expect("hook waiter release")
        .expect("hook task");
    let hook_body: Value = serde_json::from_slice(
        &hook_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    assert_eq!(hook_body["data"]["result"], "resolved_elsewhere");

    let final_event: (String, String, String) = sqlx::query_as(
        r#"SELECT event_type, source, payload
           FROM events
           WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')"#,
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(final_event.0, "approval.accepted");
    assert_eq!(final_event.1, "agent_client");
    let payload: Value = serde_json::from_str(&final_event.2).unwrap();
    assert_eq!(payload["request_event_id"], request_event_id);
    assert_eq!(payload["scope"], "once");
    assert_eq!(payload["client_session_id"], "claude_native");
    assert_eq!(payload["prompt_id"], "prompt_1");
    assert_eq!(payload["tool_name"], "Bash");
    assert_eq!(payload["tool_use_id"], "toolu_01PontiaApproval");
    assert!(payload.get("source").is_none());
    assert_eq!(
        payload.as_object().unwrap().len(),
        6,
        "only bounded correlation and decision metadata may be persisted"
    );

    let metadata: String =
        sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id = 'sess_approval'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert!(
        serde_json::from_str::<Value>(&metadata).unwrap()["interaction"].is_null(),
        "final Approval must clear the projected interaction"
    );

    let duplicate = post_otlp(state.clone(), fixture, Some("Bearer test-token")).await;
    assert_eq!(duplicate.status(), StatusCode::OK);
    let final_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(final_count, 1);
}

#[tokio::test]
async fn otlp_tool_decision_maps_all_final_decisions_without_persisting_the_otel_source() {
    for (index, decision, source, expected_event, expected_scope) in [
        (
            0,
            "accept",
            "user_permanent",
            "approval.accepted",
            Some("always"),
        ),
        (1, "accept", "config", "approval.accepted", Some("unknown")),
        (2, "accept", "hook", "approval.accepted", Some("unknown")),
        (3, "reject", "user_abort", "approval.cancelled", None),
        (4, "reject", "user_reject", "approval.rejected", None),
        (5, "reject", "config", "approval.rejected", None),
    ] {
        let state = configured_otel_state(&format!("approval-otlp-map-{index}.db")).await;
        let request_state = state.clone();
        let hook_request = tokio::spawn(async move {
            post(
                request_state,
                "/internal/v1/claude/permission-request",
                permission_request_body(),
            )
            .await
        });
        let request_event_id = approval_event_id(&state).await;
        let mut fixture = tool_decision_fixture();
        set_tool_decision_attribute(&mut fixture, "decision", decision);
        set_tool_decision_attribute(&mut fixture, "source", source);

        let response = post_otlp(state.clone(), fixture, Some("Bearer test-token")).await;
        assert_eq!(response.status(), StatusCode::OK);
        tokio::time::timeout(Duration::from_secs(2), hook_request)
            .await
            .expect("hook waiter release")
            .expect("hook task");

        let final_event: (String, String) = sqlx::query_as(
            r#"SELECT event_type, payload
               FROM events
               WHERE event_type IN (
                   'approval.accepted',
                   'approval.rejected',
                   'approval.cancelled'
               )"#,
        )
        .fetch_one(&state.db())
        .await
        .unwrap();
        assert_eq!(final_event.0, expected_event);
        let payload: Value = serde_json::from_str(&final_event.1).unwrap();
        assert_eq!(payload["request_event_id"], request_event_id);
        assert_eq!(payload.get("scope").and_then(Value::as_str), expected_scope);
        assert!(payload.get("source").is_none());
    }
}

#[tokio::test]
async fn hook_accept_uses_only_the_web_scope_delivered_by_the_current_process() {
    for (index, command, expected_scope) in [
        (0, json!({"decision": "accept_once"}), "once"),
        (
            1,
            json!({
                "decision": "always_allow",
                "permission_suggestion": {
                    "type": "addRules",
                    "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                    "behavior": "allow",
                    "destination": "localSettings"
                }
            }),
            "always",
        ),
    ] {
        let state = configured_otel_state(&format!("approval-otlp-hook-{index}.db")).await;
        let request_state = state.clone();
        let hook_request = tokio::spawn(async move {
            post(
                request_state,
                "/internal/v1/claude/permission-request",
                permission_request_body(),
            )
            .await
        });
        let request_event_id = approval_event_id(&state).await;
        let command_uri =
            format!("/external/v1/sessions/sess_approval/approvals/{request_event_id}/decision");
        let command_response =
            post_external(state.clone(), &command_uri, command, "web-hook-scope").await;
        assert_eq!(command_response.status(), StatusCode::OK);
        tokio::time::timeout(Duration::from_secs(2), hook_request)
            .await
            .expect("web command releases hook")
            .expect("hook task");

        let mut fixture = tool_decision_fixture();
        set_tool_decision_attribute(&mut fixture, "source", "hook");
        let response = post_otlp(state.clone(), fixture, Some("Bearer test-token")).await;
        assert_eq!(response.status(), StatusCode::OK);

        let payload: String =
            sqlx::query_scalar("SELECT payload FROM events WHERE event_type = 'approval.accepted'")
                .fetch_one(&state.db())
                .await
                .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&payload).unwrap()["scope"],
            expected_scope
        );
    }
}

#[tokio::test]
async fn otlp_drops_unrelated_unbound_and_uncorrelated_records_as_unknown() {
    let state = configured_otel_state("approval-otlp-drop.db").await;
    let request_state = state.clone();
    let hook_request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    approval_event_id(&state).await;

    let mut contradictory_identity = tool_decision_fixture();
    contradictory_identity["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["eventName"] =
        Value::String("claude_code.tool_result".to_string());
    assert_eq!(
        post_otlp(
            state.clone(),
            contradictory_identity,
            Some("Bearer test-token")
        )
        .await
        .status(),
        StatusCode::OK
    );

    for (key, value) in [
        ("session.id", "unbound_claude"),
        ("prompt.id", "other_prompt"),
        ("tool_name", "Write"),
    ] {
        let mut fixture = tool_decision_fixture();
        set_tool_decision_attribute(&mut fixture, key, value);
        assert_eq!(
            post_otlp(state.clone(), fixture, Some("Bearer test-token"))
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert!(!hook_request.is_finished());
    let final_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(final_count, 0);

    let response = post_otlp(
        state.clone(),
        tool_decision_fixture(),
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(2), hook_request)
        .await
        .expect("valid observation releases hook")
        .expect("hook task");
}

#[tokio::test]
async fn sequential_approvals_wait_for_the_previous_otlp_final_before_registering() {
    let state = configured_otel_state("approval-otlp-sequential.db").await;
    let first_state = state.clone();
    let first_hook = tokio::spawn(async move {
        post(
            first_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    let first_request_event_id = approval_event_id(&state).await;
    let command_uri =
        format!("/external/v1/sessions/sess_approval/approvals/{first_request_event_id}/decision");
    let command = post_external(
        state.clone(),
        &command_uri,
        json!({"decision": "accept_once"}),
        "first-web-decision",
    )
    .await;
    assert_eq!(command.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(2), first_hook)
        .await
        .expect("first web command releases hook")
        .expect("first hook task");

    let second_state = state.clone();
    let second_hook = tokio::spawn(async move {
        post(
            second_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    let requested_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'approval.requested'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(
        requested_count, 1,
        "a second durable request would make OTLP correlation ambiguous"
    );
    assert!(!second_hook.is_finished());

    let first_final = post_otlp(
        state.clone(),
        tool_decision_fixture(),
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(first_final.status(), StatusCode::OK);
    let second_request_event_id = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let Some(event_id) = sqlx::query_scalar::<_, String>(
                r#"SELECT event_id
                   FROM events
                   WHERE event_type = 'approval.requested' AND event_id != ?"#,
            )
            .bind(&first_request_event_id)
            .fetch_optional(&state.db())
            .await
            .unwrap()
            {
                break event_id;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("second Approval registration");
    assert_ne!(second_request_event_id, first_request_event_id);

    let second_final = post_otlp(
        state.clone(),
        tool_decision_fixture(),
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(second_final.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(2), second_hook)
        .await
        .expect("second OTLP final releases hook")
        .expect("second hook task");
    let final_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_type = 'approval.accepted'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert_eq!(final_count, 2);
}

#[tokio::test]
async fn otlp_requires_exactly_one_unresolved_approval_before_correlating() {
    let state = configured_otel_state("approval-otlp-ambiguous.db").await;
    let request_state = state.clone();
    let hook_request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    approval_event_id(&state).await;
    sqlx::query(
        r#"INSERT INTO events (
               event_id,
               session_id,
               turn_id,
               source,
               client_type,
               event_type,
               occurred_at,
               payload
           )
           VALUES (
               'evt_ambiguous_approval',
               'sess_approval',
               'turn_approval',
               'agent_client',
               'claude',
               'approval.requested',
               '2026-07-30T00:00:00Z',
               '{"client_session_id":"claude_native","prompt_id":"prompt_1","tool_name":"Bash","permission_suggestions":[]}'
           )"#,
    )
    .execute(&state.db())
    .await
    .unwrap();

    let ambiguous = post_otlp(
        state.clone(),
        tool_decision_fixture(),
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(ambiguous.status(), StatusCode::OK);
    assert!(!hook_request.is_finished());
    let final_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')",
    )
    .fetch_one(&state.db())
    .await
    .unwrap();
    assert_eq!(final_count, 0);

    sqlx::query("DELETE FROM events WHERE event_id = 'evt_ambiguous_approval'")
        .execute(&state.db())
        .await
        .unwrap();
    let mut without_tool_use_id = tool_decision_fixture();
    without_tool_use_id["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["attributes"]
        .as_array_mut()
        .unwrap()
        .retain(|attribute| attribute["key"] != "tool_use_id");
    let correlated = post_otlp(
        state.clone(),
        without_tool_use_id,
        Some("Bearer test-token"),
    )
    .await;
    assert_eq!(correlated.status(), StatusCode::OK);
    tokio::time::timeout(Duration::from_secs(2), hook_request)
        .await
        .expect("unambiguous observation releases hook")
        .expect("hook task");
    let payload: String =
        sqlx::query_scalar("SELECT payload FROM events WHERE event_type = 'approval.accepted'")
            .fetch_one(&state.db())
            .await
            .unwrap();
    assert!(
        serde_json::from_str::<Value>(&payload)
            .unwrap()
            .get("tool_use_id")
            .is_none(),
        "tool_use_id is optional audit metadata, not a correlation key"
    );
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

#[tokio::test]
async fn approval_commands_deliver_each_decision_without_projecting_a_final_fact() {
    for (database, decision, expected) in [
        (
            "approval-accept-once.db",
            json!({"decision": "accept_once"}),
            json!({"decision": "accept_once"}),
        ),
        (
            "approval-reject.db",
            json!({"decision": "reject"}),
            json!({"decision": "reject"}),
        ),
        (
            "approval-always-allow.db",
            json!({
                "decision": "always_allow",
                "permission_suggestion": {
                    "type": "addRules",
                    "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                    "behavior": "allow",
                    "destination": "localSettings"
                }
            }),
            json!({
                "decision": "always_allow",
                "permission_suggestion": {
                    "type": "addRules",
                    "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                    "behavior": "allow",
                    "destination": "localSettings"
                }
            }),
        ),
    ] {
        let state = TestApp::builder()
            .database_name(database)
            .external_api_token(Some("test-token".to_string()))
            .build_state()
            .await;
        sqlx::query(
            r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
               VALUES ('sess_approval', 'claude', 'busy', 'turn_approval', '{}')"#,
        )
        .execute(&state.db())
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO turns (turn_id, session_id, state, input_summary, metadata)
               VALUES ('turn_approval', 'sess_approval', 'running', 'work', '{}')"#,
        )
        .execute(&state.db())
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata)
               VALUES ('binding_approval', 'sess_approval', 'claude', '/repo', 'claude_native', '{}')"#,
        )
        .execute(&state.db())
        .await
        .unwrap();
        sqlx::query(
            r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata)
               VALUES ('sess_approval', 'claude_tui', 'rtinst_approval', '{}')"#,
        )
        .execute(&state.db())
        .await
        .unwrap();

        let request_state = state.clone();
        let hook_request = tokio::spawn(async move {
            post(
                request_state,
                "/internal/v1/claude/permission-request",
                permission_request_body(),
            )
            .await
        });
        let request_event_id = approval_event_id(&state).await;
        let uri =
            format!("/external/v1/sessions/sess_approval/approvals/{request_event_id}/decision");

        let response =
            post_external(state.clone(), &uri, decision.clone(), "approval-command").await;
        assert_eq!(response.status(), StatusCode::OK);
        let response_body: Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(response_body["data"]["request_event_id"], request_event_id);
        assert_eq!(response_body["data"]["delivered"], true);

        let hook_response = tokio::time::timeout(Duration::from_secs(2), hook_request)
            .await
            .expect("hook waiter release")
            .expect("hook task");
        let hook_body: Value = serde_json::from_slice(
            &hook_response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(hook_body["data"]["result"], expected);

        let metadata: String =
            sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id = 'sess_approval'")
                .fetch_one(&state.db())
                .await
                .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&metadata).unwrap()["interaction"]["request_event_id"],
            request_event_id
        );
        let final_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE event_type IN ('approval.accepted', 'approval.rejected', 'approval.cancelled')",
        )
        .fetch_one(&state.db())
        .await
        .unwrap();
        assert_eq!(final_count, 0);

        let duplicate = post_external(state.clone(), &uri, decision, "approval-command").await;
        assert_eq!(duplicate.status(), StatusCode::OK);
        let stale = post_external(
            state.clone(),
            &uri,
            json!({"decision": "reject"}),
            "different-command",
        )
        .await;
        assert_eq!(stale.status(), StatusCode::CONFLICT);
    }
}

#[tokio::test]
async fn always_allow_rejects_mutated_or_cross_request_suggestions_without_waking_the_waiter() {
    let state = TestApp::builder()
        .database_name("approval-exact-suggestion.db")
        .external_api_token(Some("test-token".to_string()))
        .build_state()
        .await;
    sqlx::query(
        r#"INSERT INTO sessions (session_id, client_type, state, current_turn_id, metadata)
           VALUES ('sess_approval', 'claude', 'busy', 'turn_approval', '{}')"#,
    )
    .execute(&state.db())
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO turns (turn_id, session_id, state, input_summary, metadata)
           VALUES ('turn_approval', 'sess_approval', 'running', 'work', '{}')"#,
    )
    .execute(&state.db())
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO agent_bindings (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES ('binding_approval', 'sess_approval', 'claude', '/repo', 'claude_native', '{}')"#,
    )
    .execute(&state.db())
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_instance_id, metadata)
           VALUES ('sess_approval', 'claude_tui', 'rtinst_approval', '{}')"#,
    )
    .execute(&state.db())
    .await
    .unwrap();

    let request_state = state.clone();
    let hook_request = tokio::spawn(async move {
        post(
            request_state,
            "/internal/v1/claude/permission-request",
            permission_request_body(),
        )
        .await
    });
    let request_event_id = approval_event_id(&state).await;
    let uri = format!("/external/v1/sessions/sess_approval/approvals/{request_event_id}/decision");
    let mutated = post_external(
        state.clone(),
        &uri,
        json!({
            "decision": "always_allow",
            "permission_suggestion": {
                "type": "addRules",
                "rules": [{"toolName": "Bash", "ruleContent": "pnpm *"}],
                "behavior": "allow",
                "destination": "localSettings"
            }
        }),
        "mutated",
    )
    .await;
    assert_eq!(mutated.status(), StatusCode::CONFLICT);
    assert!(!hook_request.is_finished());

    for (idempotency_key, permission_suggestion) in [
        (
            "added-field",
            json!({
                "type": "addRules",
                "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                "behavior": "allow",
                "destination": "localSettings",
                "unexpected": true
            }),
        ),
        (
            "expanded-rule",
            json!({
                "type": "addRules",
                "rules": [
                    {"toolName": "Bash", "ruleContent": "pnpm test"},
                    {"toolName": "Bash", "ruleContent": "pnpm *"}
                ],
                "behavior": "allow",
                "destination": "localSettings"
            }),
        ),
        (
            "cross-request-suggestion",
            json!({
                "type": "addRules",
                "rules": [{"toolName": "Bash", "ruleContent": "cargo test"}],
                "behavior": "allow",
                "destination": "localSettings"
            }),
        ),
    ] {
        let response = post_external(
            state.clone(),
            &uri,
            json!({
                "decision": "always_allow",
                "permission_suggestion": permission_suggestion
            }),
            idempotency_key,
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert!(!hook_request.is_finished());
    }

    let wrong_session = post_external(
        state.clone(),
        &format!("/external/v1/sessions/other/approvals/{request_event_id}/decision"),
        json!({"decision": "accept_once"}),
        "wrong-session",
    )
    .await;
    assert_eq!(wrong_session.status(), StatusCode::CONFLICT);
    assert!(!hook_request.is_finished());

    let event_only_suggestion = json!({
        "type": "addRules",
        "rules": [{"toolName": "Bash", "ruleContent": "cargo test"}],
        "behavior": "allow",
        "destination": "localSettings"
    });
    let divergent_payload = json!({
        "client_session_id": "claude_native",
        "prompt_id": "prompt_1",
        "tool_name": "Bash",
        "permission_suggestions": [event_only_suggestion.clone()]
    });
    sqlx::query("UPDATE events SET payload = ? WHERE event_id = ?")
        .bind(divergent_payload.to_string())
        .bind(&request_event_id)
        .execute(&state.db())
        .await
        .unwrap();
    let only_event_matches = post_external(
        state.clone(),
        &uri,
        json!({
            "decision": "always_allow",
            "permission_suggestion": event_only_suggestion
        }),
        "only-event-matches",
    )
    .await;
    assert_eq!(only_event_matches.status(), StatusCode::CONFLICT);
    assert!(!hook_request.is_finished());
    let only_waiter_matches = post_external(
        state.clone(),
        &uri,
        json!({
            "decision": "always_allow",
            "permission_suggestion": {
                "type": "addRules",
                "rules": [{"toolName": "Bash", "ruleContent": "pnpm test"}],
                "behavior": "allow",
                "destination": "localSettings"
            }
        }),
        "only-waiter-matches",
    )
    .await;
    assert_eq!(only_waiter_matches.status(), StatusCode::CONFLICT);
    assert!(!hook_request.is_finished());

    let accepted = post_external(
        state,
        &uri,
        json!({"decision": "reject"}),
        "reject-after-conflicts",
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::OK);
    hook_request.await.unwrap();
}

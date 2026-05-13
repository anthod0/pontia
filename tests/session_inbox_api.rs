use std::process::{Command, Stdio};

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use llmparty::{
    application::AppState,
    storage::sqlite::{connect_sqlite, run_migrations},
    transport::http,
};
use serde_json::{Value, json};
use tower::ServiceExt;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("inbox.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let db = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&db).await.expect("migrate");
    AppState {
        db,
        external_api_token: Some(TOKEN.to_string()),
        planner: Default::default(),
        graph: Default::default(),
        workspace_browser: Default::default(),
        dashboard: llmparty::transport::http::dashboard::ResolvedDashboard::local_default(),
    }
}

async fn post_json(
    state: AppState,
    uri: &str,
    idempotency_key: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(key) = idempotency_key {
        builder = builder.header("Idempotency-Key", key);
    }

    let response = http::router(state)
        .oneshot(builder.body(Body::from(body.to_string())).expect("request"))
        .await
        .expect("response");
    response_json(response).await
}

async fn post_internal_event(state: AppState, body: Value) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/internal/v1/events")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .expect("request"),
        )
        .await
        .expect("response");
    response_json(response).await
}

async fn get_json(state: AppState, uri: &str) -> (StatusCode, Value) {
    let response = http::router(state)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header(header::AUTHORIZATION, format!("Bearer {TOKEN}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    response_json(response).await
}

async fn response_json(response: axum::response::Response) -> (StatusCode, Value) {
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

async fn create_session(state: AppState) -> String {
    let (status, body) = post_json(
        state,
        "/external/v1/sessions",
        None,
        json!({"client_type":"generic"}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn submit_inbox_turn(state: AppState, session_id: &str, input: &str) -> String {
    let (status, body) = post_json(
        state,
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input": input}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn event_body(event_id: &str, event_type: &str, session_id: &str, turn_id: &str) -> Value {
    json!({
        "event_id": event_id,
        "session_id": session_id,
        "turn_id": turn_id,
        "source": "agent_adapter",
        "client_type": "generic",
        "type": event_type,
        "time": "2026-05-09T12:00:00Z",
        "seq": 10,
        "payload": {}
    })
}

struct TmuxSessionGuard {
    tmux_session: String,
}

impl TmuxSessionGuard {
    fn for_session(session_id: &str) -> Self {
        let sanitized: String = session_id
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect();
        Self {
            tmux_session: format!("llmparty_{sanitized}"),
        }
    }
}

impl Drop for TmuxSessionGuard {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &self.tmux_session])
            .stderr(Stdio::null())
            .status();
    }
}

#[tokio::test]
async fn idle_after_idle_inbox_message_dispatches_immediately() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let _guard = TmuxSessionGuard::for_session(&session_id);

    let (status, body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"continue via inbox","metadata":{"source":"test"}}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let message = &body["data"]["inbox_message"];
    assert!(message["message_id"].as_str().unwrap().starts_with("msg_"));
    assert_eq!(message["session_id"], session_id);
    assert_eq!(message["state"], "dispatched");
    assert_eq!(message["delivery_policy"], "after_idle");
    assert_eq!(message["input"]["summary"], "continue via inbox");
    assert_eq!(message["metadata"]["source"], "test");
    let turn_id = message["turn_id"].as_str().expect("turn id");

    let (turns_status, turns_body) =
        get_json(state, &format!("/external/v1/sessions/{session_id}/turns")).await;
    assert_eq!(turns_status, StatusCode::OK);
    assert_eq!(turns_body["data"]["turns"].as_array().unwrap().len(), 1);
    assert_eq!(turns_body["data"]["turns"][0]["turn_id"], turn_id);
    assert_eq!(
        turns_body["data"]["turns"][0]["metadata"]["inbox_message_id"],
        message["message_id"]
    );
}

#[tokio::test]
async fn busy_after_idle_inbox_message_waits_until_terminal_event_drains_it() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let _guard = TmuxSessionGuard::for_session(&session_id);
    let active_turn_id = submit_inbox_turn(state.clone(), &session_id, "first").await;
    let (started_status, _) = post_internal_event(
        state.clone(),
        event_body(
            "evt_inbox_started",
            "turn.started",
            &session_id,
            &active_turn_id,
        ),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK);

    let (status, body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"second"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let message_id = body["data"]["inbox_message"]["message_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(body["data"]["inbox_message"]["state"], "pending");
    assert_eq!(body["data"]["inbox_message"]["turn_id"], Value::Null);

    let (turns_status, turns_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/turns"),
    )
    .await;
    assert_eq!(turns_status, StatusCode::OK);
    assert_eq!(turns_body["data"]["turns"].as_array().unwrap().len(), 1);

    let (completed_status, _) = post_internal_event(
        state.clone(),
        event_body(
            "evt_inbox_completed",
            "turn.completed",
            &session_id,
            &active_turn_id,
        ),
    )
    .await;
    assert_eq!(completed_status, StatusCode::OK);

    let (get_status, get_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages/{message_id}"),
    )
    .await;
    assert_eq!(get_status, StatusCode::OK);
    assert_eq!(get_body["data"]["inbox_message"]["state"], "dispatched");
    assert!(
        get_body["data"]["inbox_message"]["turn_id"]
            .as_str()
            .is_some()
    );

    let (turns_status, turns_body) =
        get_json(state, &format!("/external/v1/sessions/{session_id}/turns")).await;
    assert_eq!(turns_status, StatusCode::OK);
    assert_eq!(turns_body["data"]["turns"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn idempotent_inbox_retry_returns_current_message_state_without_duplicate_turn() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let _guard = TmuxSessionGuard::for_session(&session_id);
    let uri = format!("/external/v1/sessions/{session_id}/inbox/messages");

    let first = post_json(
        state.clone(),
        &uri,
        Some("inbox-once"),
        json!({"input":"once"}),
    )
    .await;
    let second = post_json(
        state.clone(),
        &uri,
        Some("inbox-once"),
        json!({"input":"once"}),
    )
    .await;

    assert_eq!(first.0, StatusCode::CREATED);
    assert_eq!(second.0, StatusCode::OK);
    assert_eq!(first.1["data"], second.1["data"]);

    let (turns_status, turns_body) =
        get_json(state, &format!("/external/v1/sessions/{session_id}/turns")).await;
    assert_eq!(turns_status, StatusCode::OK);
    assert_eq!(turns_body["data"]["turns"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn cancel_pending_message_prevents_later_dispatch() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let _guard = TmuxSessionGuard::for_session(&session_id);
    let active_turn_id = submit_inbox_turn(state.clone(), &session_id, "first").await;
    post_internal_event(
        state.clone(),
        event_body(
            "evt_cancel_started",
            "turn.started",
            &session_id,
            &active_turn_id,
        ),
    )
    .await;

    let (_, body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"cancel me"}),
    )
    .await;
    let message_id = body["data"]["inbox_message"]["message_id"]
        .as_str()
        .unwrap();

    let (cancel_status, cancel_body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages/{message_id}/cancel"),
        None,
        json!({}),
    )
    .await;
    assert_eq!(cancel_status, StatusCode::OK);
    assert_eq!(cancel_body["data"]["inbox_message"]["state"], "cancelled");

    post_internal_event(
        state.clone(),
        event_body(
            "evt_cancel_completed",
            "turn.completed",
            &session_id,
            &active_turn_id,
        ),
    )
    .await;

    let (turns_status, turns_body) =
        get_json(state, &format!("/external/v1/sessions/{session_id}/turns")).await;
    assert_eq!(turns_status, StatusCode::OK);
    assert_eq!(turns_body["data"]["turns"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn newest_pending_interrupt_supersedes_older_pending_interrupt() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let _guard = TmuxSessionGuard::for_session(&session_id);
    post_internal_event(
        state.clone(),
        json!({
            "event_id": "evt_priority_starting",
            "session_id": session_id,
            "turn_id": null,
            "source": "agent_adapter",
            "client_type": "generic",
            "type": "session.starting",
            "time": "2026-05-09T12:00:00Z",
            "seq": 20,
            "payload": {}
        }),
    )
    .await;

    let (_, older_interrupt) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"old interrupt","delivery_policy":"interrupt_now"}),
    )
    .await;
    let (_, newer_interrupt) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"new interrupt","delivery_policy":"interrupt_now"}),
    )
    .await;

    let older_id = older_interrupt["data"]["inbox_message"]["message_id"]
        .as_str()
        .unwrap();
    let newer_id = newer_interrupt["data"]["inbox_message"]["message_id"]
        .as_str()
        .unwrap();

    let (_, old_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages/{older_id}"),
    )
    .await;
    assert_eq!(old_body["data"]["inbox_message"]["state"], "superseded");
    assert_eq!(
        old_body["data"]["inbox_message"]["superseded_by_message_id"],
        newer_id
    );

    let (_, new_body) = get_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages/{newer_id}"),
    )
    .await;
    assert_eq!(new_body["data"]["inbox_message"]["state"], "pending");

    let (turns_status, turns_body) =
        get_json(state, &format!("/external/v1/sessions/{session_id}/turns")).await;
    assert_eq!(turns_status, StatusCode::OK);
    assert!(turns_body["data"]["turns"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn interrupt_now_without_interrupt_capability_marks_message_failed() {
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let _guard = TmuxSessionGuard::for_session(&session_id);
    let active_turn_id = submit_inbox_turn(state.clone(), &session_id, "first").await;
    post_internal_event(
        state.clone(),
        event_body(
            "evt_interrupt_fail_started",
            "turn.started",
            &session_id,
            &active_turn_id,
        ),
    )
    .await;

    let (status, body) = post_json(
        state,
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"cannot interrupt","delivery_policy":"interrupt_now"}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let message = &body["data"]["inbox_message"];
    assert_eq!(message["state"], "failed");
    assert!(
        message["failure_message"]
            .as_str()
            .unwrap()
            .contains("does not support interrupt")
    );
    assert_eq!(message["turn_id"], Value::Null);
}

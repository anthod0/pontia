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

use crate::generic_client::GenericClientTestScope;

const TOKEN: &str = "test-token";

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("inbox.db")
        .external_api_token(Some(TOKEN.to_string()))
        .build_state()
        .await
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
    create_session_with_body(state, json!({"client_type":"generic"})).await
}

async fn create_session_with_body(state: AppState, body: Value) -> String {
    let (status, body) = post_json(state, "/external/v1/sessions", None, body).await;
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

async fn started_event_body(
    state: &AppState,
    event_id: &str,
    session_id: &str,
    turn_id: &str,
) -> Value {
    let runtime_instance_id: String =
        sqlx::query_scalar("SELECT runtime_instance_id FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .expect("runtime instance id");
    let mut event = event_body(event_id, "turn.started", session_id, turn_id);
    event["payload"] = json!({"runtime_instance_id": runtime_instance_id});
    event
}

#[tokio::test]
async fn dag_planner_inbox_turn_inherits_planning_context() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session_with_body(
        state.clone(),
        json!({
            "client_type": "generic",
            "metadata": {
                "dag_managed": true,
                "dag_planning_role": "planner",
                "task_id": "task_from_session",
                "planning": { "phase": "initial" }
            }
        }),
    )
    .await;

    let (status, body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"approve this plan","metadata":{"source":"dashboard_chat"}}),
    )
    .await;

    assert_eq!(status, StatusCode::CREATED);
    let turn_id = body["data"]["inbox_message"]["turn_id"]
        .as_str()
        .expect("turn id");

    let (turns_status, turns_body) =
        get_json(state, &format!("/external/v1/sessions/{session_id}/turns")).await;
    assert_eq!(turns_status, StatusCode::OK);
    let turn = turns_body["data"]["turns"]
        .as_array()
        .unwrap()
        .iter()
        .find(|turn| turn["turn_id"] == turn_id)
        .expect("dispatched inbox turn");
    assert_eq!(turn["metadata"]["dag_managed"], true);
    assert_eq!(turn["metadata"]["dag_planning_role"], "planner");
    assert_eq!(turn["metadata"]["task_id"], "task_from_session");
    assert_eq!(turn["metadata"]["planning"]["phase"], "initial");
    assert_eq!(turn["metadata"]["source"], "dashboard_chat");
    assert_eq!(
        turn["metadata"]["inbox_message_id"],
        body["data"]["inbox_message"]["message_id"]
    );
}

#[tokio::test]
async fn idle_after_idle_inbox_message_dispatches_immediately() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;

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
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let active_turn_id = submit_inbox_turn(state.clone(), &session_id, "first").await;
    let (started_status, _) = post_internal_event(
        state.clone(),
        started_event_body(&state, "evt_inbox_started", &session_id, &active_turn_id).await,
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
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
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
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let active_turn_id = submit_inbox_turn(state.clone(), &session_id, "first").await;
    post_internal_event(
        state.clone(),
        started_event_body(&state, "evt_cancel_started", &session_id, &active_turn_id).await,
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
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
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
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let active_turn_id = submit_inbox_turn(state.clone(), &session_id, "first").await;
    post_internal_event(
        state.clone(),
        started_event_body(
            &state,
            "evt_interrupt_fail_started",
            &session_id,
            &active_turn_id,
        )
        .await,
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

#[tokio::test]
async fn failed_inbox_message_can_be_dismissed_idempotently() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let active_turn_id = submit_inbox_turn(state.clone(), &session_id, "first").await;
    post_internal_event(
        state.clone(),
        started_event_body(
            &state,
            "evt_dismiss_fail_started",
            &session_id,
            &active_turn_id,
        )
        .await,
    )
    .await;
    let (_, failed_body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"cannot interrupt","delivery_policy":"interrupt_now"}),
    )
    .await;
    let message_id = failed_body["data"]["inbox_message"]["message_id"]
        .as_str()
        .expect("message id");

    let (dismiss_status, dismiss_body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages/{message_id}/dismiss"),
        None,
        json!({}),
    )
    .await;

    assert_eq!(dismiss_status, StatusCode::OK, "{dismiss_body:?}");
    assert_eq!(dismiss_body["data"]["inbox_message"]["state"], "dismissed");
    assert_eq!(
        dismiss_body["data"]["inbox_message"]["failure_message"],
        failed_body["data"]["inbox_message"]["failure_message"]
    );

    let (again_status, again_body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages/{message_id}/dismiss"),
        None,
        json!({}),
    )
    .await;
    assert_eq!(again_status, StatusCode::OK, "{again_body:?}");
    assert_eq!(again_body["data"]["inbox_message"]["state"], "dismissed");

    let (events_status, events_body) =
        get_json(state, &format!("/external/v1/sessions/{session_id}/events")).await;
    assert_eq!(events_status, StatusCode::OK);
    let dismissed_events = events_body["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|event| event["type"] == "inbox.message_dismissed")
        .count();
    assert_eq!(dismissed_events, 1);
}

#[tokio::test]
async fn pending_inbox_message_cannot_be_dismissed() {
    let _scope = GenericClientTestScope::new().await;
    let state = test_state().await;
    let session_id = create_session(state.clone()).await;
    let active_turn_id = submit_inbox_turn(state.clone(), &session_id, "first").await;
    post_internal_event(
        state.clone(),
        started_event_body(
            &state,
            "evt_pending_dismiss_started",
            &session_id,
            &active_turn_id,
        )
        .await,
    )
    .await;
    let (_, pending_body) = post_json(
        state.clone(),
        &format!("/external/v1/sessions/{session_id}/inbox/messages"),
        None,
        json!({"input":"wait until idle"}),
    )
    .await;
    let message_id = pending_body["data"]["inbox_message"]["message_id"]
        .as_str()
        .expect("message id");

    let (status, body) = post_json(
        state,
        &format!("/external/v1/sessions/{session_id}/inbox/messages/{message_id}/dismiss"),
        None,
        json!({}),
    )
    .await;

    assert_eq!(status, StatusCode::CONFLICT, "{body:?}");
    assert_eq!(body["error"]["code"], "state_conflict");
}

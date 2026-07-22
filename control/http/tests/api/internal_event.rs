use crate::test_app::TestApp;
use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use http_body_util::BodyExt;
use pontia_application::{AppState, EventIngestService};
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::{new_event_id, new_turn_id},
};
use pontia_http as http;
use pontia_storage_sqlite::repositories::runtime_bindings::{
    RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository,
};
use serde_json::{Value, json};
use tower::ServiceExt;

async fn test_state() -> AppState {
    TestApp::builder()
        .database_name("internal_event.db")
        .external_api_token(None)
        .build_state()
        .await
}

async fn create_session(state: &AppState, session_id: &str, client_type: &str) {
    EventIngestService::new(state.db())
        .ingest_event(ReportedEvent::new(
            new_event_id().to_string(),
            session_id.to_string(),
            None,
            EventSource::ExternalApi,
            client_type.to_string(),
            EventType::SessionCreated,
            json!({}),
        ))
        .await
        .expect("create session");
}

async fn bind_runtime(state: &AppState, session_id: &str, runtime_instance_id: &str) {
    SqliteRuntimeBindingRepository::new(state.db())
        .upsert_binding(RuntimeBindingUpsertRecord {
            session_id: session_id.to_string(),
            runtime_kind: "tmux".to_string(),
            runtime_instance_id: Some(runtime_instance_id.to_string()),
            start_command: None,
            launch_cwd: Some("/tmp".to_string()),
            last_seen_at: None,
            tmux_socket_path: None,
            tmux_pane_id: None,
            metadata: json!({
                "runtime_instance_id": runtime_instance_id,
                "binding_confirmed": true,
                "workspace": "/tmp"
            })
            .to_string(),
        })
        .await
        .expect("bind runtime");
}

async fn post_event(state: AppState, body: Value) -> (StatusCode, Value) {
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
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, serde_json::from_slice(&bytes).expect("json body"))
}

#[tokio::test]
async fn internal_event_api_normalizes_started_fact_into_a_domain_event() {
    let state = test_state().await;
    create_session(&state, "sess_normalized", "pi").await;
    bind_runtime(&state, "sess_normalized", "rtinst_normalized").await;

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_normalized",
            "type": "turn.started",
            "data": {
                "runtime_instance_id": "rtinst_normalized",
                "input_summary": "hello",
                "previous_leaf_id": null,
                "inbox_message_id": "msg_1"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    let event_id = body["event_id"].as_str().expect("event id");
    let turn_id = body["turn_id"].as_str().expect("turn id");
    assert!(event_id.starts_with("evt_"));
    assert!(turn_id.starts_with("turn_"));
    assert_eq!(
        turn_id[5..]
            .split('-')
            .nth(2)
            .and_then(|part| part.chars().next()),
        Some('7')
    );

    let events = EventIngestService::new(state.db())
        .list_events("sess_normalized")
        .await
        .expect("events");
    let started = events.last().expect("started event");
    assert_eq!(started.event_id, event_id);
    assert_eq!(started.turn_id.as_deref(), Some(turn_id));
    assert_eq!(started.source, EventSource::AgentAdapter);
    assert_eq!(started.client_type, "pi");
    assert_eq!(started.payload["input"]["summary"], "hello");
    assert_eq!(started.payload["metadata"]["inbox_message_id"], "msg_1");
}

#[tokio::test]
async fn internal_event_api_rejects_supplied_unknown_turn_id_for_started_fact() {
    let state = test_state().await;
    create_session(&state, "sess_unknown_started_turn", "pi").await;
    bind_runtime(
        &state,
        "sess_unknown_started_turn",
        "rtinst_unknown_started_turn",
    )
    .await;

    let (status, body) = post_event(
        state,
        json!({
            "session_id": "sess_unknown_started_turn",
            "turn_id": "turn_client_chosen",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_unknown_started_turn" }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert_eq!(body["error"]["code"], "invalid_request");
}

#[tokio::test]
async fn internal_event_api_allows_started_fact_to_reference_an_existing_turn() {
    let state = test_state().await;
    create_session(&state, "sess_existing_started_turn", "pi").await;
    bind_runtime(
        &state,
        "sess_existing_started_turn",
        "rtinst_existing_started_turn",
    )
    .await;

    let turn_id = new_turn_id().to_string();
    EventIngestService::new(state.db())
        .ingest_event(ReportedEvent::new(
            new_event_id().to_string(),
            "sess_existing_started_turn".to_string(),
            Some(turn_id.clone()),
            EventSource::ExternalApi,
            "pi".to_string(),
            EventType::TurnCreated,
            json!({}),
        ))
        .await
        .expect("create Pontia-owned turn");

    let (referenced_status, referenced) = post_event(
        state,
        json!({
            "session_id": "sess_existing_started_turn",
            "turn_id": turn_id,
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_existing_started_turn" }
        }),
    )
    .await;

    assert_eq!(referenced_status, StatusCode::OK, "{referenced:?}");
    assert_eq!(referenced["turn_id"], turn_id);
}

#[tokio::test]
async fn internal_event_api_rejects_other_creation_facts_with_unknown_supplied_turn_ids() {
    let state = test_state().await;
    create_session(&state, "sess_unknown_created_turn", "generic").await;

    for fact_type in ["turn.created", "turn.queued"] {
        let (status, body) = post_event(
            state.clone(),
            json!({
                "session_id": "sess_unknown_created_turn",
                "turn_id": format!("turn_client_chosen_{fact_type}"),
                "type": fact_type,
                "data": {}
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
        assert_eq!(body["error"]["code"], "invalid_request");
    }
}

#[tokio::test]
async fn internal_event_api_uses_returned_turn_id_for_followup_facts() {
    let state = test_state().await;
    create_session(&state, "sess_followup", "pi").await;
    bind_runtime(&state, "sess_followup", "rtinst_followup").await;
    let (_, started) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_followup",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_followup" }
        }),
    )
    .await;
    let turn_id = started["turn_id"].as_str().expect("turn id");

    for (fact_type, data) in [
        ("turn.output", json!({"output_summary":"answer"})),
        (
            "turn.completed",
            json!({"runtime_instance_id":"rtinst_followup","terminal_leaf_id":null}),
        ),
    ] {
        let (status, body) = post_event(
            state.clone(),
            json!({
                "session_id": "sess_followup",
                "turn_id": turn_id,
                "type": fact_type,
                "data": data
            }),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert_eq!(body["turn_id"], turn_id);
    }

    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .expect("turn query")
        .expect("turn");
    assert_eq!(turn.output_summary.as_deref(), Some("answer"));
    assert_eq!(turn.state.to_string(), "completed");
}

#[tokio::test]
async fn internal_event_api_derives_client_type_and_source_from_session_and_fact() {
    let state = test_state().await;
    create_session(&state, "sess_ready", "pi").await;
    bind_runtime(&state, "sess_ready", "rtinst_ready").await;

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_ready",
            "type": "session.ready",
            "data": {
                "runtime_instance_id": "rtinst_ready",
                "client_session_key": "native-pi-session"
            }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let events = EventIngestService::new(state.db())
        .list_events("sess_ready")
        .await
        .expect("events");
    let ready = events.last().expect("ready");
    assert_eq!(ready.source, EventSource::AgentClient);
    assert_eq!(ready.client_type, "pi");
}

#[tokio::test]
async fn internal_event_api_rejects_unknown_sessions_and_missing_followup_turn_ids() {
    let state = test_state().await;
    let (status, _) = post_event(
        state.clone(),
        json!({"session_id":"sess_unknown","type":"session.ready","data":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    create_session(&state, "sess_missing_turn", "generic").await;
    let (status, body) = post_event(
        state,
        json!({"session_id":"sess_missing_turn","type":"turn.output","data":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
}

#[tokio::test]
async fn internal_event_api_rejects_followups_for_unknown_or_other_session_turns() {
    let state = test_state().await;
    create_session(&state, "sess_turn_owner", "pi").await;
    create_session(&state, "sess_turn_intruder", "pi").await;
    bind_runtime(&state, "sess_turn_owner", "rtinst_owner").await;
    bind_runtime(&state, "sess_turn_intruder", "rtinst_intruder").await;

    let (unknown_status, unknown_body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_turn_owner",
            "turn_id": "turn_missing",
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await;
    assert_eq!(unknown_status, StatusCode::CONFLICT, "{unknown_body:?}");

    let (started_status, started) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_turn_owner",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_owner" }
        }),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK, "{started:?}");
    let turn_id = started["turn_id"].as_str().unwrap();
    let (cross_session_status, cross_session_body) = post_event(
        state,
        json!({
            "session_id": "sess_turn_intruder",
            "turn_id": turn_id,
            "type": "turn.output",
            "data": { "output_summary": "not mine" }
        }),
    )
    .await;
    assert_eq!(
        cross_session_status,
        StatusCode::CONFLICT,
        "{cross_session_body:?}"
    );
}

#[tokio::test]
async fn internal_event_api_rejects_client_owned_domain_fields() {
    let state = test_state().await;
    create_session(&state, "sess_owned_fields", "generic").await;
    let (status, body) = post_event(
        state,
        json!({
            "event_id": "evt_client",
            "session_id": "sess_owned_fields",
            "source": "agent_client",
            "client_type": "generic",
            "type": "session.message_updated",
            "time": "2026-01-01T00:00:00Z",
            "data": {}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
}

#[tokio::test]
async fn internal_event_api_validates_context_usage_and_truncates_output() {
    let state = test_state().await;
    create_session(&state, "sess_validation", "generic").await;

    let (status, _) = post_event(
        state.clone(),
        json!({
            "session_id":"sess_validation",
            "type":"session.context_usage_updated",
            "data":{"context_usage":{"usage_ratio":2}}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (_, started) = post_event(
        state.clone(),
        json!({"session_id":"sess_validation","type":"turn.started","data":{}}),
    )
    .await;
    let turn_id = started["turn_id"].as_str().expect("turn id");
    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id":"sess_validation",
            "turn_id":turn_id,
            "type":"turn.output",
            "data":{"output":{"summary":"x".repeat(500)}}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .expect("turn query")
        .expect("turn");
    assert_eq!(turn.output_summary.expect("summary").chars().count(), 200);
}

#[tokio::test]
async fn internal_event_api_keeps_session_message_updates_volatile() {
    let state = test_state().await;
    create_session(&state, "sess_volatile", "generic").await;
    let before = EventIngestService::new(state.db())
        .list_events("sess_volatile")
        .await
        .expect("events")
        .len();
    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id":"sess_volatile",
            "type":"session.message_updated",
            "data":{"reason":"append"}
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    let after = EventIngestService::new(state.db())
        .list_events("sess_volatile")
        .await
        .expect("events")
        .len();
    assert_eq!(after, before);
}

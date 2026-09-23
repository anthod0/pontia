use axum::http::StatusCode;
use pontia_application::EventIngestService;
use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::{new_event_id, new_turn_id},
};
use serde_json::json;

use super::fixture::{bind_runtime, create_session, post_event, test_state};

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
            "payload": {
                "runtime_instance_id": "rtinst_normalized",
                "input_summary": "hello",
                "previous_leaf_id": null,
                "inbox_message_id": "msg_1"
            }
        }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["accepted"], true);
    assert_eq!(body["duplicate"], false);
    assert_eq!(body["session_id"], "sess_normalized");
    assert_eq!(body["state_version"], 2);
    assert_eq!(body["warnings"], json!([]));
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
        .ingest_reported_event(ReportedEvent::new(
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
async fn internal_event_api_accepts_agent_client_reported_turn_interrupted() {
    let state = test_state().await;
    create_session(&state, "sess_interrupted", "pi").await;
    bind_runtime(&state, "sess_interrupted", "rtinst_interrupted").await;

    let (started_status, started) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_interrupted",
            "type": "turn.started",
            "data": { "runtime_instance_id": "rtinst_interrupted" }
        }),
    )
    .await;
    assert_eq!(started_status, StatusCode::OK, "{started:?}");
    let turn_id = started["turn_id"].as_str().expect("turn id");

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_interrupted",
            "turn_id": turn_id,
            "type": "turn.interrupted",
            "data": { "runtime_instance_id": "rtinst_interrupted" }
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body:?}");

    let turn = EventIngestService::new(state.db())
        .get_turn(turn_id)
        .await
        .expect("turn query")
        .expect("turn");
    assert_eq!(turn.state.to_string(), "interrupted");
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

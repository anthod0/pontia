use axum::http::StatusCode;
use pontia_application::EventIngestService;
use serde_json::json;

use super::fixture::{bind_runtime, create_session, post_event, test_state};

#[tokio::test]
async fn internal_event_api_rejects_pontia_owned_event_types() {
    let state = test_state().await;
    create_session(&state, "sess_owned_event", "generic").await;

    for fact_type in ["session.created", "turn.dispatch_failed", "turn.abandoned"] {
        let (status, body) = post_event(
            state.clone(),
            json!({
                "session_id": "sess_owned_event",
                "turn_id": "turn_owned_event",
                "type": fact_type,
                "data": {}
            }),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("owned by the Pontia control plane"),
            "{body:?}"
        );
    }
}

#[tokio::test]
async fn internal_event_api_rejects_timeline_boundary_as_an_unknown_field() {
    let state = test_state().await;

    let (status, body) = post_event(
        state,
        json!({
            "session_id": "sess_unknown_field",
            "type": "session.ready",
            "data": {},
            "timeline_boundary": null
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown field `timeline_boundary`")),
        "{body:?}"
    );
}

#[tokio::test]
async fn internal_event_api_rejects_removed_timeline_item_events() {
    let state = test_state().await;

    let (status, body) = post_event(
        state,
        json!({
            "session_id": "sess_removed_timeline_event",
            "turn_id": "turn_removed_timeline_event",
            "type": "turn.timeline_item",
            "data": {}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert_eq!(body["error"]["code"], "invalid_request");
    assert!(
        body["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("unknown event type: turn.timeline_item")),
        "{body:?}"
    );
}

#[tokio::test]
async fn internal_event_api_rejection_does_not_broadcast() {
    let state = test_state().await;
    create_session(&state, "sess_rejected_broadcast", "generic").await;
    let mut subscriber = state.agent_events().subscribe();

    let (status, body) = post_event(
        state.clone(),
        json!({
            "session_id": "sess_rejected_broadcast",
            "type": "turn.completed",
            "data": {}
        }),
    )
    .await;

    assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
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

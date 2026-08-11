use pontia_core::domain::{
    EventSource, EventType, ProjectionState, ReportedEvent, SessionState, TurnState,
};
use serde_json::json;

use crate::fixture::{event, service};

#[tokio::test]
async fn ingest_persists_events_and_updates_projections() {
    let service = service().await;

    service
        .ingest_reported_event(event("evt_1", EventType::SessionCreated, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_reported_event(event("evt_2", EventType::SessionReady, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_3",
            EventType::TurnCreated,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_4",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();
    let result = service
        .ingest_reported_event(event(
            "evt_5",
            EventType::TurnCompleted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();

    assert_eq!(result.state_version, 5);
    assert!(!result.duplicate);

    let session = service.get_session("sess_1").await.unwrap().unwrap();
    let turn = service.get_turn("turn_1").await.unwrap().unwrap();
    let events = service.list_events("sess_1").await.unwrap();

    assert_eq!(session.state, SessionState::Idle);
    assert_eq!(session.current_turn_id.as_deref(), Some("turn_1"));
    assert_eq!(session.state_version, 5);
    assert_eq!(turn.state, TurnState::Completed);
    assert_eq!(events.len(), 5);
}

#[tokio::test]
async fn ingest_persists_turn_input_and_output_summaries() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_summary_session",
            EventType::SessionCreated,
            "sess_summary",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(ReportedEvent::new(
            "evt_summary_input".to_string(),
            "sess_summary".to_string(),
            Some("turn_summary".to_string()),
            EventSource::ExternalApi,
            "generic".to_string(),
            EventType::TurnCreated,
            json!({ "input": { "summary": "inspect summaries" } }),
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(ReportedEvent::new(
            "evt_summary_output".to_string(),
            "sess_summary".to_string(),
            Some("turn_summary".to_string()),
            EventSource::ExternalApi,
            "generic".to_string(),
            EventType::TurnOutput,
            json!({ "output": { "summary": "summaries persisted" } }),
        ))
        .await
        .unwrap();

    let turn = service.get_turn("turn_summary").await.unwrap().unwrap();
    assert_eq!(turn.input_summary.as_deref(), Some("inspect summaries"));
    assert_eq!(turn.output_summary.as_deref(), Some("summaries persisted"));
}

#[tokio::test]
async fn session_started_keeps_projection_starting_until_ready() {
    let service = service().await;

    service
        .ingest_reported_event(event(
            "evt_started_created",
            EventType::SessionCreated,
            "sess_started",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_started",
            EventType::SessionStarted,
            "sess_started",
            None,
        ))
        .await
        .unwrap();

    let session = service.get_session("sess_started").await.unwrap().unwrap();
    assert_eq!(session.state, SessionState::Starting);
}

#[tokio::test]
async fn storage_rejects_second_active_turn() {
    let service = service().await;

    service
        .ingest_reported_event(event("evt_1", EventType::SessionCreated, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_reported_event(event("evt_2", EventType::SessionReady, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_3",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();

    let result = service
        .ingest_reported_event(event(
            "evt_4",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_2"),
        ))
        .await;

    assert!(result.is_err());
    assert!(service.get_turn("turn_2").await.unwrap().is_none());
    assert_eq!(
        service
            .get_session("sess_1")
            .await
            .unwrap()
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
}

#[tokio::test]
async fn session_event_turn_context_does_not_create_a_turn() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_session",
            EventType::SessionCreated,
            "sess_1",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_session_context",
            EventType::SessionTitleUpdated,
            "sess_1",
            Some("turn_context"),
        ))
        .await
        .unwrap();

    assert!(service.get_turn("turn_context").await.unwrap().is_none());
    let events = service.list_events("sess_1").await.unwrap();
    assert_eq!(events[1].turn_id.as_deref(), Some("turn_context"));
}

#[tokio::test]
async fn replay_preserves_turn_identity_and_state_without_ordinals() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_session",
            EventType::SessionCreated,
            "sess_1",
            None,
        ))
        .await
        .unwrap();

    for (event_id, event_type, turn_id) in [
        ("evt_1", EventType::TurnStarted, "turn_1"),
        ("evt_2", EventType::TurnCompleted, "turn_1"),
        ("evt_3", EventType::TurnStarted, "turn_2"),
        ("evt_4", EventType::TurnCompleted, "turn_2"),
    ] {
        service
            .ingest_reported_event(event(event_id, event_type, "sess_1", Some(turn_id)))
            .await
            .unwrap();
    }

    let events = service.list_events("sess_1").await.unwrap();
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    assert_eq!(replay.turn("turn_1").unwrap().state, TurnState::Completed);
    assert_eq!(replay.turn("turn_2").unwrap().state, TurnState::Completed);
    assert_eq!(replay.turn("turn_1").unwrap().session_id, "sess_1");
    assert_eq!(replay.turn("turn_2").unwrap().session_id, "sess_1");
}

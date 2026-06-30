use pontia_core::domain::{
    DomainEvent, EventSource, EventType, ProjectionState, SessionState, TurnState,
};
use serde_json::json;

fn event(event_type: EventType, session_id: &str, turn_id: Option<&str>) -> DomainEvent {
    DomainEvent::new(
        format!("evt_{:?}_{:?}", event_type, turn_id).replace(['.', '"', ' '], "_"),
        session_id.to_string(),
        turn_id.map(str::to_string),
        EventSource::ExternalApi,
        "generic".to_string(),
        event_type,
        json!({}),
    )
}

#[test]
fn reducer_projects_session_lifecycle_and_turn_busy_idle() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionStarting, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCreated, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnQueued, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();

    assert_eq!(
        projection.session("sess_1").unwrap().state,
        SessionState::Busy
    );
    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
    assert_eq!(projection.turn("turn_1").unwrap().state, TurnState::Running);

    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();

    assert_eq!(
        projection.session("sess_1").unwrap().state,
        SessionState::Idle
    );
    assert_eq!(projection.session("sess_1").unwrap().current_turn_id, None);
    assert_eq!(
        projection.turn("turn_1").unwrap().state,
        TurnState::Completed
    );
}

#[test]
fn reducer_does_not_let_late_events_change_terminal_session_or_turn() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCreated, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::SessionExited, "sess_1", None))
        .unwrap();

    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();

    assert_eq!(
        projection.session("sess_1").unwrap().state,
        SessionState::Exited
    );
    assert_eq!(
        projection.turn("turn_1").unwrap().state,
        TurnState::Completed
    );
}

#[test]
fn reducer_derives_missing_session_title_from_first_turn_input() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&DomainEvent::new(
            "evt_turn_started_title".to_string(),
            "sess_1".to_string(),
            Some("turn_1".to_string()),
            EventSource::AgentAdapter,
            "pi".to_string(),
            EventType::TurnStarted,
            json!({ "input": { "summary": "  inspect TUI-created task titles\nwith details" } }),
        ))
        .unwrap();

    assert_eq!(
        projection.session("sess_1").unwrap().title.as_deref(),
        Some("inspect TUI-created task titles")
    );
}

#[test]
fn reducer_rejects_second_active_turn_in_same_session() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCreated, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();

    let result = projection.apply(&event(EventType::TurnStarted, "sess_1", Some("turn_2")));

    assert!(result.is_err());
    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
    assert!(projection.turn("turn_2").is_none());
}

#[test]
fn runtime_binding_is_auxiliary_not_domain_transition() {
    let mut projection = ProjectionState::default();

    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    let before = projection.session("sess_1").unwrap().clone();

    projection.record_runtime_binding("sess_1", "tmux:abc");

    assert_eq!(projection.session("sess_1").unwrap(), &before);
}

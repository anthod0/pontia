use pontia_core::domain::{DomainEvent, EventSource, EventType, ProjectionState, SessionState};
use serde_json::json;

use crate::fixture::event;

#[test]
fn approval_interaction_is_orthogonal_to_turn_lifecycle_and_clears_on_terminal_turn() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();

    let requested = DomainEvent::new(
        "evt_approval_requested".to_string(),
        "sess_1".to_string(),
        Some("turn_1".to_string()),
        EventSource::AgentClient,
        "claude".to_string(),
        EventType::ApprovalRequested,
        json!({"tool_name": "Bash"}),
    );
    projection.apply(&requested).unwrap();

    let session = projection.session("sess_1").unwrap();
    assert_eq!(session.state, SessionState::Busy);
    assert_eq!(
        session.metadata["interaction"],
        json!({
            "type": "approval",
            "state": "awaiting",
            "request_event_id": "evt_approval_requested"
        })
    );

    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();
    let session = projection.session("sess_1").unwrap();
    assert_eq!(session.state, SessionState::Idle);
    assert!(session.metadata.get("interaction").is_none());
}

#[test]
fn approval_interaction_clears_when_the_session_exits_or_errors() {
    for (terminal_event, terminal_state) in [
        (EventType::SessionExited, SessionState::Exited),
        (EventType::SessionError, SessionState::Error),
    ] {
        let mut projection = ProjectionState::default();
        projection
            .apply(&event(EventType::SessionCreated, "sess_1", None))
            .unwrap();
        projection
            .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
            .unwrap();
        projection
            .apply(&DomainEvent::new(
                "evt_approval_requested".to_string(),
                "sess_1".to_string(),
                Some("turn_1".to_string()),
                EventSource::AgentClient,
                "claude".to_string(),
                EventType::ApprovalRequested,
                json!({"tool_name": "Bash"}),
            ))
            .unwrap();

        projection
            .apply(&event(terminal_event, "sess_1", None))
            .unwrap();

        let session = projection.session("sess_1").unwrap();
        assert_eq!(session.state, terminal_state);
        assert!(session.metadata.get("interaction").is_none());
    }
}

#[test]
fn approval_events_require_a_turn_without_becoming_turn_lifecycle_events() {
    assert!(EventType::ApprovalRequested.requires_turn_id());
    assert!(!EventType::ApprovalRequested.is_turn_event());

    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    let error = projection
        .apply(&event(EventType::ApprovalRequested, "sess_1", None))
        .expect_err("approval must bind an existing Turn");
    assert!(error.to_string().contains("requires turn_id"));
}

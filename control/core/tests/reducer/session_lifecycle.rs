use pontia_core::domain::{
    DomainEvent, EventSource, EventType, ProjectionState, SessionState, TurnState,
};
use serde_json::json;

use crate::fixture::event;

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
    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
    assert_eq!(
        projection.turn("turn_1").unwrap().state,
        TurnState::Completed
    );
}

#[test]
fn reducer_keeps_current_branch_leaf_across_terminal_turn_and_session_events() {
    for terminal_event in [
        EventType::TurnCompleted,
        EventType::TurnFailed,
        EventType::TurnInterrupted,
    ] {
        let mut projection = ProjectionState::default();
        projection
            .apply(&event(EventType::SessionCreated, "sess_1", None))
            .unwrap();
        projection
            .apply(&event(EventType::SessionReady, "sess_1", None))
            .unwrap();
        projection
            .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
            .unwrap();
        projection
            .apply(&event(terminal_event, "sess_1", Some("turn_1")))
            .unwrap();

        assert_eq!(
            projection
                .session("sess_1")
                .unwrap()
                .current_turn_id
                .as_deref(),
            Some("turn_1")
        );
    }

    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::SessionError, "sess_1", None))
        .unwrap();

    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
}

#[test]
fn reducer_keeps_the_latest_started_turn_as_the_current_branch_leaf() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();

    projection
        .apply(&event(EventType::TurnCreated, "sess_1", Some("turn_2")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnQueued, "sess_1", Some("turn_2")))
        .unwrap();
    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );

    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_2")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_2")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnFailed, "sess_1", Some("turn_1")))
        .unwrap();

    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_2")
    );
}

#[test]
fn reducer_session_exit_abandons_execution_without_replacing_the_branch_leaf() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionReady, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();
    projection
        .apply(&event(EventType::TurnQueued, "sess_1", Some("turn_2")))
        .unwrap();

    projection
        .apply(&event(EventType::SessionExited, "sess_1", None))
        .unwrap();

    assert_eq!(
        projection.turn("turn_2").unwrap().state,
        TurnState::Abandoned
    );
    assert_eq!(
        projection
            .session("sess_1")
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
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
fn reducer_rejects_a_changed_session_during_replay() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionCreated, "sess_1", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnCompleted, "sess_1", Some("turn_1")))
        .unwrap();

    let changed = DomainEvent::new(
        "evt_changed_session".to_string(),
        "sess_2".to_string(),
        Some("turn_1".to_string()),
        EventSource::ExternalApi,
        "generic".to_string(),
        EventType::TurnOutput,
        json!({}),
    );
    let error = projection
        .apply(&changed)
        .expect_err("Turn session must be immutable during replay");
    assert!(error.to_string().contains("immutable session_id"));
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

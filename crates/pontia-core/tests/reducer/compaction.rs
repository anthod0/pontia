use pontia_core::domain::{EventType, ProjectionState, SessionState};
use serde_json::json;

use crate::fixture::event;

#[test]
fn manual_compaction_projects_busy_then_its_outcome_without_changing_turns() {
    for (kind, aborted, expected) in [
        (
            EventType::SessionCompactionCompleted,
            false,
            SessionState::Idle,
        ),
        (
            EventType::SessionCompactionFailed,
            false,
            SessionState::Idle,
        ),
        (
            EventType::SessionCompactionFailed,
            true,
            SessionState::Interrupted,
        ),
    ] {
        let mut projection = ProjectionState::default();
        for (kind, turn) in [
            (EventType::SessionReady, None),
            (EventType::TurnStarted, Some("turn")),
            (EventType::TurnCompleted, Some("turn")),
        ] {
            projection.apply(&event(kind, "session", turn)).unwrap();
        }
        let previous_turn = projection.turn("turn").unwrap().clone();
        let mut started = event(EventType::SessionCompactionStarted, "session", None);
        started.payload = json!({"operation_id":"compact", "trigger":"manual"});
        projection.apply(&started).unwrap();
        assert_eq!(
            projection.session("session").unwrap().state,
            SessionState::Busy
        );

        let mut terminal = event(kind, "session", None);
        terminal.payload = json!({"operation_id":"compact", "trigger":"manual", "aborted":aborted});
        projection.apply(&terminal).unwrap();
        let session = projection.session("session").unwrap();
        assert_eq!(session.state, expected);
        assert_eq!(session.current_turn_id.as_deref(), Some("turn"));
        assert_eq!(projection.turn("turn"), Some(&previous_turn));
        assert_eq!(projection.turns().count(), 1);
    }
}

#[test]
fn non_manual_compaction_does_not_change_session_execution_state() {
    for trigger in ["threshold", "overflow", "unknown"] {
        for lifecycle in [
            EventType::SessionReady,
            EventType::TurnStarted,
            EventType::TurnInterrupted,
        ] {
            let mut projection = ProjectionState::default();
            projection
                .apply(&event(EventType::SessionReady, "session", None))
                .unwrap();
            if lifecycle != EventType::SessionReady {
                projection
                    .apply(&event(EventType::TurnStarted, "session", Some("turn")))
                    .unwrap();
                if lifecycle == EventType::TurnInterrupted {
                    projection
                        .apply(&event(lifecycle, "session", Some("turn")))
                        .unwrap();
                }
            }
            let before = projection.session("session").unwrap().clone();
            for kind in [
                EventType::SessionCompactionStarted,
                EventType::SessionCompactionCompleted,
                EventType::SessionCompactionFailed,
            ] {
                let mut fact = event(kind, "session", None);
                fact.payload = json!({"operation_id":"compact", "trigger":trigger, "aborted":true});
                projection.apply(&fact).unwrap();
                assert_eq!(projection.session("session"), Some(&before));
            }
        }
    }
}

#[test]
fn late_manual_compaction_facts_do_not_reopen_an_exited_session() {
    let mut projection = ProjectionState::default();
    projection
        .apply(&event(EventType::SessionExited, "session", None))
        .unwrap();
    for kind in [
        EventType::SessionCompactionStarted,
        EventType::SessionCompactionCompleted,
        EventType::SessionCompactionFailed,
    ] {
        let mut fact = event(kind, "session", None);
        fact.payload = json!({"operation_id":"compact", "trigger":"manual", "aborted":true});
        projection.apply(&fact).unwrap();
        assert_eq!(
            projection.session("session").unwrap().state,
            SessionState::Exited
        );
    }
}

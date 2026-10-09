use crate::fixture::event;
use pontia_core::domain::{
    EventType, ProjectionState, SessionExecutionLifetime, SessionState, TurnState,
};

#[test]
fn ending_a_subscription_keeps_the_native_turn_open_for_its_terminal_fact() {
    let mut projection =
        ProjectionState::default().with_execution_lifetime(SessionExecutionLifetime::Subscription);
    for (kind, turn) in [
        (EventType::SessionCreated, None),
        (EventType::SessionReady, None),
        (EventType::TurnStarted, Some("turn")),
        (EventType::SessionExited, None),
    ] {
        projection.apply(&event(kind, "session", turn)).unwrap();
    }
    assert_eq!(
        projection.session("session").unwrap().state,
        SessionState::Exited
    );
    assert_eq!(projection.turn("turn").unwrap().state, TurnState::Running);
    projection
        .apply(&event(EventType::TurnCompleted, "session", Some("turn")))
        .unwrap();
    assert_eq!(projection.turn("turn").unwrap().state, TurnState::Completed);
    assert_eq!(
        projection.session("session").unwrap().state,
        SessionState::Exited
    );
}

#[test]
fn a_native_turn_observed_after_subscription_exit_does_not_resume_the_session() {
    let mut projection =
        ProjectionState::default().with_execution_lifetime(SessionExecutionLifetime::Subscription);
    projection
        .apply(&event(EventType::SessionCreated, "session", None))
        .unwrap();
    projection
        .apply(&event(EventType::SessionExited, "session", None))
        .unwrap();
    projection
        .apply(&event(EventType::TurnStarted, "session", Some("turn")))
        .unwrap();
    assert_eq!(projection.turn("turn").unwrap().state, TurnState::Running);
    assert_eq!(
        projection.session("session").unwrap().state,
        SessionState::Exited
    );
    projection
        .apply(&event(EventType::TurnInterrupted, "session", Some("turn")))
        .unwrap();
    assert_eq!(
        projection.turn("turn").unwrap().state,
        TurnState::Interrupted
    );
    assert_eq!(
        projection.session("session").unwrap().state,
        SessionState::Exited
    );
}

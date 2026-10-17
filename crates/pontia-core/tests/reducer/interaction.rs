fn event(kind: EventType, session: &str, turn: Option<&str>) -> pontia_core::domain::DomainEvent {
    let mut fact = super::fixture::event(kind, session, turn);
    fact.occurred_at = time::OffsetDateTime::UNIX_EPOCH;
    fact
}
use pontia_core::domain::{EventType, ProjectionState, SessionState, TurnState};
use serde_json::json;

fn running() -> ProjectionState {
    let mut state = ProjectionState::default();
    state
        .apply(&event(EventType::SessionCreated, "s", None))
        .unwrap();
    state
        .apply(&event(EventType::TurnStarted, "s", Some("t")))
        .unwrap();
    state
}

fn interaction(kind: EventType, id: &str) -> pontia_core::domain::DomainEvent {
    let mut fact = event(kind, "s", Some("t"));
    fact.payload = json!({"interaction_id": id});
    fact
}

#[test]
fn independently_unblocks_multiple_interactions_and_preserves_running_turn() {
    let mut state = running();
    for id in ["a", "b", "a"] {
        state
            .apply(&interaction(EventType::InteractionBlocked, id))
            .unwrap();
    }
    for kind in [EventType::TurnOutput, EventType::SessionReady] {
        state
            .apply(&event(
                kind,
                "s",
                (kind == EventType::TurnOutput).then_some("t"),
            ))
            .unwrap();
        assert_eq!(
            state.session("s").unwrap().state,
            SessionState::WaitingForInteraction
        );
    }
    for _ in 0..2 {
        state
            .apply(&interaction(EventType::InteractionUnblocked, "a"))
            .unwrap();
        assert_eq!(
            state.session("s").unwrap().state,
            SessionState::WaitingForInteraction
        );
    }
    state
        .apply(&interaction(EventType::InteractionUnblocked, "b"))
        .unwrap();
    assert_eq!(state.session("s").unwrap().state, SessionState::Busy);
    assert_eq!(state.turn("t").unwrap().state, TurnState::Running);
    // The same Interaction may become blocking again after a new client fact.
    state
        .apply(&interaction(EventType::InteractionBlocked, "a"))
        .unwrap();
    assert_eq!(
        state.session("s").unwrap().state,
        SessionState::WaitingForInteraction
    );
}

#[test]
fn terminal_execution_facts_supersede_waiting_and_late_interaction_events() {
    for (kind, expected) in [
        (EventType::TurnCompleted, SessionState::Idle),
        (EventType::TurnInterrupted, SessionState::Interrupted),
        (EventType::SessionExited, SessionState::Exited),
        (EventType::SessionError, SessionState::Error),
    ] {
        let mut state = running();
        let blocked = interaction(EventType::InteractionBlocked, "a");
        state.apply(&blocked).unwrap();
        state
            .apply(&event(kind, "s", kind.is_turn_event().then_some("t")))
            .unwrap();
        assert_eq!(state.session("s").unwrap().state, expected);
        state.apply(&blocked).unwrap();
        state
            .apply(&interaction(EventType::InteractionUnblocked, "a"))
            .unwrap();
        assert_eq!(state.session("s").unwrap().state, expected);
    }
}

#[test]
fn ignores_out_of_order_block_after_a_newer_unblock() {
    let mut state = running();
    let blocked = interaction(EventType::InteractionBlocked, "a");
    let mut unblocked = interaction(EventType::InteractionUnblocked, "a");
    unblocked.occurred_at += time::Duration::seconds(1);
    state.apply(&unblocked).unwrap();
    state.apply(&blocked).unwrap();
    assert_eq!(state.session("s").unwrap().state, SessionState::Busy);
}

#[test]
fn session_level_block_does_not_create_a_turn() {
    let mut state = ProjectionState::default();
    state
        .apply(&event(EventType::SessionCreated, "s", None))
        .unwrap();
    state
        .apply(&event(EventType::SessionReady, "s", None))
        .unwrap();
    let mut fact = interaction(EventType::InteractionBlocked, "a");
    fact.turn_id = None;
    state.apply(&fact).unwrap();
    assert_eq!(
        state.session("s").unwrap().state,
        SessionState::WaitingForInteraction
    );
    assert_eq!(state.turns().count(), 0);
    fact.event_type = EventType::InteractionUnblocked;
    state.apply(&fact).unwrap();
    assert_eq!(state.session("s").unwrap().state, SessionState::Idle);
}

#[test]
fn resuming_does_not_clear_waiting_and_unblock_respects_latest_startup_fact() {
    let mut state = ProjectionState::default();
    state
        .apply(&event(EventType::SessionCreated, "s", None))
        .unwrap();
    state
        .apply(&event(EventType::SessionStarting, "s", None))
        .unwrap();
    let mut fact = interaction(EventType::InteractionBlocked, "a");
    fact.turn_id = None;
    state.apply(&fact).unwrap();
    state
        .apply(&event(EventType::SessionResuming, "s", None))
        .unwrap();
    assert_eq!(
        state.session("s").unwrap().state,
        SessionState::WaitingForInteraction
    );
    fact.event_type = EventType::InteractionUnblocked;
    state.apply(&fact).unwrap();
    assert_eq!(state.session("s").unwrap().state, SessionState::Starting);
    state
        .apply(&event(EventType::SessionReady, "s", None))
        .unwrap();
    assert_eq!(state.session("s").unwrap().state, SessionState::Idle);
}

#[test]
fn rejects_invalid_interaction_identity_and_foreign_turn() {
    let mut state = running();
    for id in [json!(null), json!(4), json!("  ")] {
        let mut fact = interaction(EventType::InteractionBlocked, "a");
        fact.payload = json!({"interaction_id": id});
        assert!(state.apply(&fact).is_err());
        assert_eq!(state.session("s").unwrap().state, SessionState::Busy);
    }
    let mut fact = interaction(EventType::InteractionBlocked, "a");
    fact.turn_id = Some("unknown".into());
    assert!(state.apply(&fact).is_err());
}

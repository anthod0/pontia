use super::fixture::{event, service};
use pontia_core::domain::{DomainEvent, EventType, ProjectionState, SessionState};
use serde_json::json;

#[tokio::test]
async fn durable_blocking_projection_matches_history_replay_after_every_commit() {
    let service = service().await;
    let sequence = [
        (EventType::SessionCreated, None, None, SessionState::Created),
        (EventType::TurnStarted, Some("t"), None, SessionState::Busy),
        (
            EventType::InteractionBlocked,
            Some("t"),
            Some("a"),
            SessionState::WaitingForInteraction,
        ),
        (
            EventType::InteractionBlocked,
            None,
            Some("b"),
            SessionState::WaitingForInteraction,
        ),
        (
            EventType::TurnOutput,
            Some("t"),
            None,
            SessionState::WaitingForInteraction,
        ),
        (
            EventType::SessionReady,
            None,
            None,
            SessionState::WaitingForInteraction,
        ),
        (
            EventType::InteractionUnblocked,
            Some("t"),
            Some("a"),
            SessionState::WaitingForInteraction,
        ),
        (
            EventType::InteractionUnblocked,
            Some("t"),
            Some("a"),
            SessionState::WaitingForInteraction,
        ),
        (
            EventType::InteractionUnblocked,
            None,
            Some("b"),
            SessionState::Busy,
        ),
        (
            EventType::InteractionBlocked,
            None,
            Some("a"),
            SessionState::WaitingForInteraction,
        ),
        (
            EventType::TurnInterrupted,
            Some("t"),
            None,
            SessionState::Interrupted,
        ),
        (
            EventType::InteractionUnblocked,
            None,
            Some("a"),
            SessionState::Interrupted,
        ),
        (
            EventType::InteractionBlocked,
            Some("t"),
            Some("late"),
            SessionState::Interrupted,
        ),
        (EventType::SessionExited, None, None, SessionState::Exited),
        (
            EventType::InteractionBlocked,
            None,
            Some("after-exit"),
            SessionState::Exited,
        ),
    ];
    let mut replay = ProjectionState::default();
    for (index, (kind, turn, interaction, expected)) in sequence.into_iter().enumerate() {
        let mut fact = event(&format!("event-{index}"), kind, "s", turn);
        fact.occurred_at = time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(index as i64);
        if let Some(id) = interaction {
            fact.payload = json!({"interaction_id": id});
        }
        let canonical = DomainEvent::from(fact.clone());
        let result = service.ingest_reported_event(fact.clone()).await.unwrap();
        assert!(!result.duplicate);
        // Retrying the same durable fact cannot change either state or cursor.
        let duplicate = service.ingest_reported_event(fact).await.unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(result.state_version, duplicate.state_version);
        replay.apply(&canonical).unwrap();
        let persisted = service.get_session("s").await.unwrap().unwrap();
        assert_eq!(persisted.state, expected, "after {kind}");
        assert_eq!(
            persisted.state,
            replay.session("s").unwrap().state,
            "replay after {kind}"
        );
        assert_eq!(service.list_events("s").await.unwrap().len(), index + 1);
    }
    let mut full_replay = ProjectionState::default();
    for fact in service.list_events("s").await.unwrap() {
        full_replay.apply(&fact).unwrap();
    }
    assert_eq!(
        full_replay.session("s").unwrap().state,
        SessionState::Exited
    );
}

#[tokio::test]
async fn delayed_turn_completion_and_unrelated_events_match_full_history_replay() {
    let service = service().await;
    let mut replay = ProjectionState::default();
    for (index, kind, seconds) in [
        (0, EventType::SessionCreated, 0),
        (1, EventType::TurnStarted, 1),
        (2, EventType::InteractionBlocked, 20),
        (3, EventType::TurnCompleted, 10),
        (4, EventType::SessionTitleUpdated, 30),
    ] {
        let mut fact = event(
            &format!("skew-{index}"),
            kind,
            "s",
            (kind.is_turn_event() || kind.is_interaction_event()).then_some("t"),
        );
        fact.occurred_at = time::OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(seconds);
        if kind.is_interaction_event() {
            fact.payload = json!({"interaction_id":"a"});
        }
        replay.apply(&DomainEvent::from(fact.clone())).unwrap();
        service.ingest_reported_event(fact).await.unwrap();
        assert_eq!(
            service.get_session("s").await.unwrap().unwrap().state,
            replay.session("s").unwrap().state
        );
    }
    assert_eq!(
        service.get_session("s").await.unwrap().unwrap().state,
        SessionState::Idle
    );
}

#[tokio::test]
async fn invalid_block_is_not_committed_and_cannot_change_session_projection() {
    let service = service().await;
    service
        .ingest_reported_event(event("created", EventType::SessionCreated, "s", None))
        .await
        .unwrap();
    let before = service.get_session("s").await.unwrap().unwrap();
    let bad = event("invalid", EventType::InteractionBlocked, "s", None);
    assert!(service.ingest_reported_event(bad).await.is_err());
    assert_eq!(service.get_session("s").await.unwrap().unwrap(), before);
    assert_eq!(service.list_events("s").await.unwrap().len(), 1);
}

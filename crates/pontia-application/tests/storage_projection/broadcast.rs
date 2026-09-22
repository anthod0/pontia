use pontia_application::{AgentEventBroker, EventIngestService};
use pontia_core::domain::{EventType, TurnState};

use crate::fixture::{event, service_with_agent_events, test_pool};

#[tokio::test]
async fn committed_agent_event_is_broadcast_after_its_projection_is_queryable() {
    let (service, broker) = service_with_agent_events().await;
    service
        .ingest_reported_event(event(
            "evt_broadcast_created",
            EventType::SessionCreated,
            "sess_broadcast",
            None,
        ))
        .await
        .unwrap();
    let mut subscriber = broker.subscribe();

    service
        .ingest_reported_event(event(
            "evt_broadcast_started",
            EventType::TurnStarted,
            "sess_broadcast",
            Some("turn_broadcast"),
        ))
        .await
        .unwrap();

    let broadcast = subscriber.recv().await.expect("committed event");
    assert_eq!(broadcast.event_id, "evt_broadcast_started");
    assert_eq!(broadcast.event_type, EventType::TurnStarted);
    let turn = service
        .get_turn("turn_broadcast")
        .await
        .unwrap()
        .expect("projected turn");
    assert_eq!(turn.state, TurnState::Running);
}

#[tokio::test]
async fn subscribers_receive_committed_turn_and_session_terminal_facts() {
    let (service, broker) = service_with_agent_events().await;
    for (session_id, turn_id) in [
        ("sess_completed", Some("turn_completed")),
        ("sess_failed", Some("turn_failed")),
        ("sess_interrupted", Some("turn_interrupted")),
        ("sess_exited", None),
        ("sess_started", None),
    ] {
        service
            .ingest_reported_event(event(
                &format!("evt_{session_id}_created"),
                EventType::SessionCreated,
                session_id,
                None,
            ))
            .await
            .unwrap();
        if let Some(turn_id) = turn_id {
            service
                .ingest_reported_event(event(
                    &format!("evt_{turn_id}_started"),
                    EventType::TurnStarted,
                    session_id,
                    Some(turn_id),
                ))
                .await
                .unwrap();
        }
    }
    let mut subscriber = broker.subscribe();

    for (event_id, event_type, session_id, turn_id) in [
        (
            "evt_started",
            EventType::TurnStarted,
            "sess_started",
            Some("turn_started"),
        ),
        (
            "evt_completed",
            EventType::TurnCompleted,
            "sess_completed",
            Some("turn_completed"),
        ),
        (
            "evt_failed",
            EventType::TurnFailed,
            "sess_failed",
            Some("turn_failed"),
        ),
        (
            "evt_interrupted",
            EventType::TurnInterrupted,
            "sess_interrupted",
            Some("turn_interrupted"),
        ),
        ("evt_exited", EventType::SessionExited, "sess_exited", None),
    ] {
        service
            .ingest_reported_event(event(event_id, event_type, session_id, turn_id))
            .await
            .unwrap();
        let broadcast = subscriber.recv().await.expect("committed event");
        assert_eq!(broadcast.event_id, event_id);
        assert_eq!(broadcast.event_type, event_type);
    }
}

#[tokio::test]
async fn rejected_agent_event_is_not_broadcast() {
    let (service, broker) = service_with_agent_events().await;
    service
        .ingest_reported_event(event(
            "evt_rejected_created",
            EventType::SessionCreated,
            "sess_rejected",
            None,
        ))
        .await
        .unwrap();
    let mut subscriber = broker.subscribe();

    let result = service
        .ingest_reported_event(event(
            "evt_rejected_completed",
            EventType::TurnCompleted,
            "sess_rejected",
            None,
        ))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn persistence_failure_is_not_broadcast() {
    let (pool, _pontia_home) = test_pool("closed-agent-events.db").await;
    let broker = AgentEventBroker::default();
    let service = EventIngestService::new(pool.clone()).with_agent_events(broker.clone());
    let mut subscriber = broker.subscribe();
    pool.close().await;

    let result = service
        .ingest_reported_event(event(
            "evt_persistence_failure",
            EventType::SessionCreated,
            "sess_persistence_failure",
            None,
        ))
        .await;

    assert!(result.is_err());
    assert!(matches!(
        subscriber.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
}

use pontia_core::{
    Error,
    domain::{EventType, ProjectionState, TurnTopology},
};

use crate::fixture::{event, service};

#[tokio::test]
async fn topology_enrichment_is_atomic_durable_and_replayable() {
    let service = service().await;
    service
        .ingest_reported_event(event(
            "evt_topology_session",
            EventType::SessionCreated,
            "sess_topology",
            None,
        ))
        .await
        .unwrap();

    service
        .ingest_event_with_topology(
            event(
                "evt_topology_root",
                EventType::TurnStarted,
                "sess_topology",
                Some("turn_01900000-0000-7000-8000-000000000001"),
            ),
            TurnTopology::Root,
        )
        .await
        .unwrap();
    service
        .ingest_reported_event(event(
            "evt_topology_root_completed",
            EventType::TurnCompleted,
            "sess_topology",
            Some("turn_01900000-0000-7000-8000-000000000001"),
        ))
        .await
        .unwrap();
    service
        .ingest_event_with_topology(
            event(
                "evt_topology_child",
                EventType::TurnStarted,
                "sess_topology",
                Some("turn_01900000-0000-7000-8000-000000000002"),
            ),
            TurnTopology::linked("turn_01900000-0000-7000-8000-000000000001"),
        )
        .await
        .unwrap();

    assert_eq!(
        service
            .get_turn("turn_01900000-0000-7000-8000-000000000001")
            .await
            .unwrap()
            .unwrap()
            .topology,
        TurnTopology::Root
    );
    assert_eq!(
        service
            .get_turn("turn_01900000-0000-7000-8000-000000000002")
            .await
            .unwrap()
            .unwrap()
            .topology,
        TurnTopology::linked("turn_01900000-0000-7000-8000-000000000001")
    );

    let events = service.list_events("sess_topology").await.unwrap();
    assert_eq!(events[1].topology, Some(TurnTopology::Root));
    assert_eq!(
        events[3].topology,
        Some(TurnTopology::linked(
            "turn_01900000-0000-7000-8000-000000000001"
        ))
    );
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    assert_eq!(
        replay
            .turn("turn_01900000-0000-7000-8000-000000000002")
            .unwrap()
            .topology,
        TurnTopology::linked("turn_01900000-0000-7000-8000-000000000001")
    );
}

#[tokio::test]
async fn invalid_turn_facts_are_rejected_before_persistence() {
    let service = service().await;
    for session in ["session", "other_session"] {
        service
            .ingest_reported_event(event(
                &format!("created_{session}"),
                EventType::SessionCreated,
                session,
                None,
            ))
            .await
            .unwrap();
    }
    for (turn, session) in [("turn_1", "session"), ("turn_4", "other_session")] {
        service
            .ingest_event_with_topology(
                event(
                    &format!("started_{turn}"),
                    EventType::TurnStarted,
                    session,
                    Some(turn),
                ),
                TurnTopology::Root,
            )
            .await
            .unwrap();
        service
            .ingest_reported_event(event(
                &format!("completed_{turn}"),
                EventType::TurnCompleted,
                session,
                Some(turn),
            ))
            .await
            .unwrap();
    }
    let before = service.list_events("session").await.unwrap();
    for (label, turn, parent) in [
        ("missing_parent", "turn_2", "missing"),
        ("self_parent", "turn_2", "turn_2"),
        ("forward_parent", "turn_0", "turn_1"),
        ("cross_session_parent", "turn_5", "turn_4"),
        ("conflicting_topology", "turn_1", "turn_0"),
    ] {
        let result = service
            .ingest_event_with_topology(
                event(label, EventType::TurnStarted, "session", Some(turn)),
                TurnTopology::linked(parent),
            )
            .await;
        assert!(
            matches!(result, Err(Error::Domain(_))),
            "{label}: {result:?}"
        );
    }
    let missing_id = service
        .ingest_reported_event(event(
            "missing_turn_id",
            EventType::TurnCompleted,
            "session",
            None,
        ))
        .await;
    assert!(matches!(missing_id, Err(Error::Domain(_))));
    assert_eq!(
        service.list_events("session").await.unwrap().len(),
        before.len()
    );
    for turn in ["turn_0", "turn_2", "turn_5"] {
        assert!(service.get_turn(turn).await.unwrap().is_none());
    }
    let existing = service.get_turn("turn_1").await.unwrap().unwrap();
    assert_eq!(existing.topology, TurnTopology::Root);
}

use pontia_core::domain::{EventType, ProjectionState, TurnTopology};

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

    let invalid = service
        .ingest_event_with_topology(
            event(
                "evt_topology_invalid",
                EventType::TurnStarted,
                "sess_topology",
                Some("turn_01900000-0000-7000-8000-000000000003"),
            ),
            TurnTopology::linked("turn_01900000-0000-7000-8000-000000000003"),
        )
        .await;
    assert!(invalid.is_err());
    assert!(
        service
            .get_turn("turn_01900000-0000-7000-8000-000000000003")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        service
            .list_events("sess_topology")
            .await
            .unwrap()
            .iter()
            .all(|event| event.event_id != "evt_topology_invalid")
    );
}

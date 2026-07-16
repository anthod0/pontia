use pontia_application::EventIngestService;
use pontia_core::domain::{
    EventSource, EventType, ProjectionState, ReportedEvent, SessionState, TurnState, TurnTopology,
};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

async fn service() -> EventIngestService {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join("m1.db");
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    EventIngestService::new(pool)
}

fn event(
    event_id: &str,
    event_type: EventType,
    session_id: &str,
    turn_id: Option<&str>,
) -> ReportedEvent {
    ReportedEvent::new(
        event_id.to_string(),
        session_id.to_string(),
        turn_id.map(str::to_string),
        EventSource::ExternalApi,
        "generic".to_string(),
        event_type,
        json!({}),
    )
}

#[tokio::test]
async fn ingest_persists_events_and_updates_projections() {
    let service = service().await;

    service
        .ingest_event(event("evt_1", EventType::SessionCreated, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_event(event("evt_2", EventType::SessionReady, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_3",
            EventType::TurnCreated,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_4",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();
    let result = service
        .ingest_event(event(
            "evt_5",
            EventType::TurnCompleted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();

    assert_eq!(result.state_version, 5);
    assert!(!result.duplicate);

    let session = service.get_session("sess_1").await.unwrap().unwrap();
    let turn = service.get_turn("turn_1").await.unwrap().unwrap();
    let events = service.list_events("sess_1").await.unwrap();

    assert_eq!(session.state, SessionState::Idle);
    assert_eq!(session.current_turn_id, None);
    assert_eq!(session.state_version, 5);
    assert_eq!(turn.state, TurnState::Completed);
    assert_eq!(events.len(), 5);
}

#[tokio::test]
async fn ingest_persists_turn_input_and_output_summaries() {
    let service = service().await;
    service
        .ingest_event(event(
            "evt_summary_session",
            EventType::SessionCreated,
            "sess_summary",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_event(ReportedEvent::new(
            "evt_summary_input".to_string(),
            "sess_summary".to_string(),
            Some("turn_summary".to_string()),
            EventSource::ExternalApi,
            "generic".to_string(),
            EventType::TurnCreated,
            json!({ "input": { "summary": "inspect summaries" } }),
        ))
        .await
        .unwrap();
    service
        .ingest_event(ReportedEvent::new(
            "evt_summary_output".to_string(),
            "sess_summary".to_string(),
            Some("turn_summary".to_string()),
            EventSource::ExternalApi,
            "generic".to_string(),
            EventType::TurnOutput,
            json!({ "output": { "summary": "summaries persisted" } }),
        ))
        .await
        .unwrap();

    let turn = service.get_turn("turn_summary").await.unwrap().unwrap();
    assert_eq!(turn.input_summary.as_deref(), Some("inspect summaries"));
    assert_eq!(turn.output_summary.as_deref(), Some("summaries persisted"));
}

#[tokio::test]
async fn session_started_keeps_projection_starting_until_ready() {
    let service = service().await;

    service
        .ingest_event(event(
            "evt_started_created",
            EventType::SessionCreated,
            "sess_started",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_started",
            EventType::SessionStarted,
            "sess_started",
            None,
        ))
        .await
        .unwrap();

    let session = service.get_session("sess_started").await.unwrap().unwrap();
    assert_eq!(session.state, SessionState::Starting);
}

#[tokio::test]
async fn duplicate_event_id_is_idempotent() {
    let service = service().await;
    let first = event("evt_same", EventType::SessionCreated, "sess_1", None);

    let first_result = service.ingest_event(first.clone()).await.unwrap();
    let second_result = service.ingest_event(first).await.unwrap();

    assert!(!first_result.duplicate);
    assert!(second_result.duplicate);
    assert_eq!(first_result.state_version, second_result.state_version);
    assert_eq!(service.list_events("sess_1").await.unwrap().len(), 1);
}

#[tokio::test]
async fn storage_rejects_second_active_turn() {
    let service = service().await;

    service
        .ingest_event(event("evt_1", EventType::SessionCreated, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_event(event("evt_2", EventType::SessionReady, "sess_1", None))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_3",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();

    let result = service
        .ingest_event(event(
            "evt_4",
            EventType::TurnStarted,
            "sess_1",
            Some("turn_2"),
        ))
        .await;

    assert!(result.is_err());
    assert!(service.get_turn("turn_2").await.unwrap().is_none());
    assert_eq!(
        service
            .get_session("sess_1")
            .await
            .unwrap()
            .unwrap()
            .current_turn_id
            .as_deref(),
        Some("turn_1")
    );
}

#[tokio::test]
async fn session_event_turn_context_does_not_allocate_a_turn_index() {
    let service = service().await;
    service
        .ingest_event(event(
            "evt_session",
            EventType::SessionCreated,
            "sess_1",
            None,
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_session_context",
            EventType::SessionTitleUpdated,
            "sess_1",
            Some("turn_context"),
        ))
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_turn",
            EventType::TurnCompleted,
            "sess_1",
            Some("turn_1"),
        ))
        .await
        .unwrap();

    let events = service.list_events("sess_1").await.unwrap();
    assert_eq!(events[1].turn_index, None);
    assert_eq!(
        service
            .get_turn("turn_1")
            .await
            .unwrap()
            .unwrap()
            .turn_index,
        1
    );
}

#[tokio::test]
async fn allocates_monotonic_indexes_and_reuses_them_for_later_events_and_replay() {
    let service = service().await;
    service
        .ingest_event(event(
            "evt_session",
            EventType::SessionCreated,
            "sess_1",
            None,
        ))
        .await
        .unwrap();

    for (event_id, event_type, turn_id) in [
        ("evt_1", EventType::TurnStarted, "turn_1"),
        ("evt_2", EventType::TurnCompleted, "turn_1"),
        ("evt_3", EventType::TurnStarted, "turn_2"),
        ("evt_4", EventType::TurnCompleted, "turn_2"),
    ] {
        service
            .ingest_event(event(event_id, event_type, "sess_1", Some(turn_id)))
            .await
            .unwrap();
    }

    assert_eq!(
        service
            .get_turn("turn_1")
            .await
            .unwrap()
            .unwrap()
            .turn_index,
        1
    );
    assert_eq!(
        service
            .get_turn("turn_2")
            .await
            .unwrap()
            .unwrap()
            .turn_index,
        2
    );

    let events = service.list_events("sess_1").await.unwrap();
    let turn_one_indexes: Vec<_> = events
        .iter()
        .filter(|event| event.turn_id.as_deref() == Some("turn_1"))
        .map(|event| event.turn_index)
        .collect();
    assert_eq!(turn_one_indexes, vec![Some(1), Some(1)]);

    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    assert_eq!(replay.turn("turn_1").unwrap().turn_index, 1);
    assert_eq!(replay.turn("turn_2").unwrap().turn_index, 2);
}

#[tokio::test]
async fn topology_enrichment_is_atomic_durable_and_replayable() {
    let service = service().await;
    service
        .ingest_event(event(
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
                Some("turn_root"),
            ),
            TurnTopology::Root,
        )
        .await
        .unwrap();
    service
        .ingest_event(event(
            "evt_topology_root_completed",
            EventType::TurnCompleted,
            "sess_topology",
            Some("turn_root"),
        ))
        .await
        .unwrap();
    service
        .ingest_event_with_topology(
            event(
                "evt_topology_child",
                EventType::TurnStarted,
                "sess_topology",
                Some("turn_child"),
            ),
            TurnTopology::linked("turn_root"),
        )
        .await
        .unwrap();

    assert_eq!(
        service
            .get_turn("turn_root")
            .await
            .unwrap()
            .unwrap()
            .topology,
        TurnTopology::Root
    );
    assert_eq!(
        service
            .get_turn("turn_child")
            .await
            .unwrap()
            .unwrap()
            .topology,
        TurnTopology::linked("turn_root")
    );

    let events = service.list_events("sess_topology").await.unwrap();
    assert_eq!(events[1].topology, Some(TurnTopology::Root));
    assert_eq!(events[3].topology, Some(TurnTopology::linked("turn_root")));
    let mut replay = ProjectionState::default();
    for event in &events {
        replay.apply(event).unwrap();
    }
    assert_eq!(
        replay.turn("turn_child").unwrap().topology,
        TurnTopology::linked("turn_root")
    );

    let invalid = service
        .ingest_event_with_topology(
            event(
                "evt_topology_invalid",
                EventType::TurnStarted,
                "sess_topology",
                Some("turn_invalid"),
            ),
            TurnTopology::linked("turn_invalid"),
        )
        .await;
    assert!(invalid.is_err());
    assert!(service.get_turn("turn_invalid").await.unwrap().is_none());
    assert!(
        service
            .list_events("sess_topology")
            .await
            .unwrap()
            .iter()
            .all(|event| event.event_id != "evt_topology_invalid")
    );
}

#[tokio::test]
async fn concurrent_first_events_receive_distinct_session_local_indexes() {
    let service = service().await;
    service
        .ingest_event(event(
            "evt_session",
            EventType::SessionCreated,
            "sess_1",
            None,
        ))
        .await
        .unwrap();

    let left = service.clone();
    let right = service.clone();
    let (left_result, right_result) = tokio::join!(
        left.ingest_event(event(
            "evt_left",
            EventType::TurnCompleted,
            "sess_1",
            Some("turn_left"),
        )),
        right.ingest_event(event(
            "evt_right",
            EventType::TurnCompleted,
            "sess_1",
            Some("turn_right"),
        )),
    );
    left_result.unwrap();
    right_result.unwrap();

    let mut indexes = vec![
        service
            .get_turn("turn_left")
            .await
            .unwrap()
            .unwrap()
            .turn_index,
        service
            .get_turn("turn_right")
            .await
            .unwrap()
            .unwrap()
            .turn_index,
    ];
    indexes.sort_unstable();
    assert_eq!(indexes, vec![1, 2]);
}

use pontia::{
    application::EventIngestService,
    domain::{DomainEvent, EventSource, EventType, SessionState, TurnState},
    storage::sqlite::{connect_sqlite, run_migrations},
};
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
) -> DomainEvent {
    DomainEvent::new(
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

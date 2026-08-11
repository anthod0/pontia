use pontia_application::{AgentEventBroker, EventIngestService};
use pontia_core::domain::{EventSource, EventType, ReportedEvent};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use sqlx::SqlitePool;

pub(super) async fn test_pool(database_name: &str) -> SqlitePool {
    let dir = tempfile::tempdir().expect("tempdir");
    let db_path = dir.path().join(database_name);
    let _kept_dir = dir.keep();
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    pool
}

pub(super) async fn service() -> EventIngestService {
    EventIngestService::new(test_pool("m1.db").await)
}

pub(super) async fn service_with_agent_events() -> (EventIngestService, AgentEventBroker) {
    let broker = AgentEventBroker::default();
    (
        EventIngestService::new(test_pool("agent-events.db").await)
            .with_agent_events(broker.clone()),
        broker,
    )
}

pub(super) fn event(
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

use pontia_application::{AgentEventBroker, EventIngestService};
use pontia_core::domain::{EventSource, EventType, ReportedEvent};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use sqlx::SqlitePool;
use std::{ops::Deref, sync::Arc};

#[derive(Clone)]
pub(super) struct TestService {
    service: EventIngestService,
    _pontia_home: Arc<tempfile::TempDir>,
}

impl Deref for TestService {
    type Target = EventIngestService;

    fn deref(&self) -> &Self::Target {
        &self.service
    }
}

pub(super) async fn test_pool(database_name: &str) -> (SqlitePool, Arc<tempfile::TempDir>) {
    let pontia_home = Arc::new(tempfile::tempdir().expect("Pontia home"));
    let db_path = pontia_home.path().join(database_name);
    let database_url = format!("sqlite://{}", db_path.display());
    let pool = connect_sqlite(&database_url).await.expect("connect");
    run_migrations(&pool).await.expect("migrate");
    (pool, pontia_home)
}

pub(super) async fn service() -> TestService {
    let (pool, pontia_home) = test_pool("m1.db").await;
    TestService {
        service: EventIngestService::new(pool),
        _pontia_home: pontia_home,
    }
}

pub(super) async fn service_with_agent_events() -> (TestService, AgentEventBroker) {
    let broker = AgentEventBroker::default();
    let (pool, pontia_home) = test_pool("agent-events.db").await;
    (
        TestService {
            service: EventIngestService::new(pool).with_agent_events(broker.clone()),
            _pontia_home: pontia_home,
        },
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

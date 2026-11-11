use pontia_agent_control::{AgentControl, PontiaEvent, PontiaEventSource, PontiaEventType};
use pontia_application::AppState;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

#[tokio::test]
async fn token_override_keeps_control_notifications_and_shutdown_shared() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let state = AppState::builder(AgentControl::builder(pool, root.path().into()).build()).build();
    let authenticated = state.with_external_api_token(Some("token".into()));
    let mut events = state.control().agent_events().subscribe();
    let mut shutdown = state.control().shutdown().subscribe();
    authenticated
        .control()
        .event_ingest_service()
        .ingest_pontia_event(PontiaEvent::new(
            "session",
            None,
            PontiaEventSource::ExternalApi,
            "generic",
            PontiaEventType::SessionCreated,
            json!({"title": "Shared control"}),
        ))
        .await
        .unwrap();
    let event = events.try_recv().unwrap();
    assert_eq!(event.session_id, "session");
    assert_eq!(
        event.event_type,
        pontia_core::domain::EventType::SessionCreated
    );
    assert_eq!(
        state
            .control()
            .queries()
            .get_session("session")
            .await
            .unwrap()
            .unwrap()
            .title
            .as_deref(),
        Some("Shared control")
    );
    assert_eq!(state.external_api_token(), None);
    assert_eq!(authenticated.external_api_token(), Some("token"));
    authenticated.control().shutdown().notify();
    shutdown.changed().await.unwrap();
    assert!(*shutdown.borrow());
}

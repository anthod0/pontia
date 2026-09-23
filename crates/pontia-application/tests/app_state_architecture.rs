use pontia_application::client_contract::{RuntimeBindingBehavior, test_registration};
use pontia_application::clients::ClientRegistry;
use pontia_application::{AppState, PontiaEvent, PontiaEventSource, PontiaEventType};
use pontia_core::domain::EventType;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

#[tokio::test]
async fn runtime_registration_and_resume_publish_to_the_application_broker() {
    let root = tempfile::tempdir().unwrap();
    let db = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&db).await.unwrap();
    static SPEC: std::sync::OnceLock<pontia_application::client_contract::AgentClientSpec> =
        std::sync::OnceLock::new();
    let mut registration = test_registration();
    registration.spec = SPEC.get_or_init(|| {
        let mut spec = pontia_application::client_contract::TEST_SPEC.clone();
        spec.adapter.runtime_binding = RuntimeBindingBehavior::Named {
            runtime_kind: "test",
        };
        spec
    });
    let mut clients = ClientRegistry::default();
    clients.register(registration);
    let state = AppState::builder(db, root.path().into())
        .clients(clients)
        .build();
    let mut notices = state.agent_events().subscribe();
    let request = json!({
        "client_type":"generic", "client_session_key":"native", "launch_cwd":root.path(),
        "tmux":{"socket_path":root.path().join("missing.sock"),"pane_id":"%1"}
    });
    let registered = state
        .runtime_bindings()
        .upsert(serde_json::from_value(request.clone()).unwrap())
        .await
        .unwrap();
    let session = registered["session"]["session_id"].as_str().unwrap();
    for kind in [
        EventType::SessionCreated,
        EventType::SessionStarting,
        EventType::SessionStarted,
    ] {
        let event = notices
            .try_recv()
            .expect("registration must publish through shared dependencies");
        assert_eq!(event.event_type, kind);
        assert_eq!(event.session_id, session);
        assert!(
            state
                .event_ingest_service()
                .list_events(session)
                .await
                .unwrap()
                .iter()
                .any(|stored| stored.event_id == event.event_id)
        );
    }
    assert!(notices.try_recv().is_err());
    state
        .event_ingest_service()
        .ingest_pontia_event(PontiaEvent::new(
            session,
            None,
            PontiaEventSource::RuntimeManager,
            "generic",
            PontiaEventType::SessionExited,
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(
        notices.try_recv().unwrap().event_type,
        EventType::SessionExited
    );
    state
        .runtime_bindings()
        .upsert(serde_json::from_value(request).unwrap())
        .await
        .unwrap();
    for kind in [EventType::SessionResuming, EventType::SessionStarted] {
        assert_eq!(
            notices
                .try_recv()
                .expect("resume must publish through shared dependencies")
                .event_type,
            kind
        );
    }
    assert!(notices.try_recv().is_err());
}

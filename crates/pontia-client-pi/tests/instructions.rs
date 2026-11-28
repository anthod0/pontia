use pontia_agent_control::{
    SessionInstructionsService, clients::ClientRegistry, standalone_session_instructions,
};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

#[tokio::test]
async fn instructions_compose_only_the_requested_sources() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_pi::registration(None));
    let service = SessionInstructionsService::new(pool).with_clients(clients);
    let standalone = standalone_session_instructions();
    for (system, include_standalone, expected) in [
        (None, true, Some(standalone.to_owned())),
        (
            Some("caller-marker"),
            true,
            Some(format!("{standalone}\n\ncaller-marker")),
        ),
        (Some("Hello {{name}}"), false, Some("Hello {{name}}".into())),
        (None, false, None),
    ] {
        assert_eq!(
            service.resolve("pi", system, include_standalone).unwrap(),
            expected.map(|prompt| json!({"system_prompt":prompt}))
        );
    }
    for invalid in ["", " \n"] {
        assert!(matches!(
            service.resolve("pi", Some(invalid), false),
            Err(pontia_core::Error::Domain(_))
        ));
    }
}

#[tokio::test]
async fn caller_instructions_require_client_injection_capability() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_agent_control::client_contract::test_registration());
    let service = SessionInstructionsService::new(pool).with_clients(clients);
    assert!(matches!(
        service.resolve("generic", Some("Keep context"), false),
        Err(pontia_core::Error::Unsupported(_))
    ));
    assert_eq!(service.resolve("generic", None, true).unwrap(), None);
}

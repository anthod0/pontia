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
        (Some("caller-marker"), false, Some("caller-marker".into())),
        (None, false, None),
    ] {
        assert_eq!(
            service.resolve("pi", system, include_standalone).unwrap(),
            expected.map(|prompt| json!({"system_prompt":prompt}))
        );
    }
    for invalid in ["", " \n", "Hello {{name}}", "name}}"] {
        assert!(matches!(
            service.resolve("pi", Some(invalid), false),
            Err(pontia_core::Error::Domain(_))
        ));
    }
}

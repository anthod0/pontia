use pontia_application::{AppState, agent_clients::AgentClientService, clients::ClientRegistry};
use pontia_client_codex::configuration::CodexConfiguration;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn new_sessions_capture_tui_configuration_without_reconfiguring_existing_sessions() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    let mut client = pontia_client_codex::registration();
    client.configuration = Some(Arc::new(CodexConfiguration::new(
        json!({"tui_command":"file-codex"}),
    )));
    let mut clients = ClientRegistry::default();
    clients.register(client);
    let app = AppState::builder(pool.clone(), root.path().into())
        .clients(clients.clone())
        .build();
    let service = AgentClientService::new(pool.clone(), clients);
    let create = || async {
        app.session_commands()
            .create_session(
                serde_json::from_value(json!({"client_type":"codex","workspace":root.path()}))
                    .unwrap(),
            )
            .await
    };
    let first = create().await.unwrap().session_id().unwrap().to_owned();
    service
        .save(
            "codex",
            "default",
            None,
            Some(json!({"tui_command":"database-codex"})),
        )
        .await
        .unwrap();
    let second = create().await.unwrap().session_id().unwrap().to_owned();
    for (session, command) in [(&first, "file-codex"), (&second, "database-codex")] {
        let stored: String = sqlx::query_scalar(
            "SELECT json_extract(metadata,'$.codex_tui_command') FROM sessions WHERE session_id=?",
        )
        .bind(session)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored, command);
    }
    service
        .save("codex", "default", Some(false), None)
        .await
        .unwrap();
    assert!(matches!(
        create().await,
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
    assert!(
        service
            .save(
                "codex",
                "default",
                Some(true),
                Some(json!({"tui_command":null}))
            )
            .await
            .is_err()
    );
    service.delete("codex", "default").await.unwrap();
    let third = create().await.unwrap().session_id().unwrap().to_owned();
    let stored: String = sqlx::query_scalar(
        "SELECT json_extract(metadata,'$.codex_tui_command') FROM sessions WHERE session_id=?",
    )
    .bind(third)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, "file-codex");
    pool.close().await;
}

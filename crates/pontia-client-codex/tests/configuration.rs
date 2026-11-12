use pontia_agent_control::{
    AgentControl, agent_clients::AgentClientService, clients::ClientRegistry,
};
use pontia_client_codex::configuration::CodexConfiguration;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn codex_configuration_preserves_admission_without_consuming_legacy_tui_settings() {
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
    let app = AgentControl::builder(pool.clone(), root.path().into())
        .clients(clients.clone())
        .build();
    let service = AgentClientService::new(pool.clone(), clients);
    let entries = service.list().await.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].client_type, "codex");
    assert!(!entries[0].enabled);
    let create = || async {
        app.session_commands()
            .create_session(
                serde_json::from_value(json!({"client_type":"codex","workspace":root.path()}))
                    .unwrap(),
            )
            .await
    };
    assert!(matches!(
        create().await,
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
    service
        .save("codex", "default", Some(true), None)
        .await
        .unwrap();
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
    for session in [&first, &second] {
        let metadata: String =
            sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id=?")
                .bind(session)
                .fetch_one(&pool)
                .await
                .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert!(metadata.get("codex_tui_command").is_none());
    }
    service
        .save("codex", "default", Some(false), None)
        .await
        .unwrap();
    assert!(matches!(
        create().await,
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
    service
        .save(
            "codex",
            "default",
            Some(true),
            Some(json!({"tui_command":null})),
        )
        .await
        .unwrap();
    assert!(service.get("codex", "default").await.unwrap().enabled);
    create().await.unwrap();
    service.delete("codex", "default").await.unwrap();
    assert!(matches!(
        create().await,
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
    // Saving configuration without an enabled field still enables a new entry.
    service
        .save("codex", "default", None, Some(json!({})))
        .await
        .unwrap();
    create().await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_runtimes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    pool.close().await;
}

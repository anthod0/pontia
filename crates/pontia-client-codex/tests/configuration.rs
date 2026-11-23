use pontia_agent_control::{
    AgentControl, agent_clients::AgentClientService, clients::ClientRegistry,
};
use pontia_client_codex::configuration::CodexConfiguration;
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

#[tokio::test]
async fn tool_selection_denials_and_mcp_disabling_are_rejected_before_creation() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_codex::registration());
    AgentClientService::new(pool.clone(), clients.clone())
        .save("codex", "default", true)
        .await
        .unwrap();
    let app = AgentControl::builder(pool.clone(), root.path().into())
        .clients(clients)
        .build();
    for policy in [
        json!({"tools":["exec_command"]}),
        json!({"tools":[]}),
        json!({"disabled_tools":["apply_patch"]}),
        json!({"disable_mcp":true}),
    ] {
        let request = serde_json::from_value(
            json!({"client_type":"codex", "workspace":root.path(), "execution_policy":policy}),
        )
        .unwrap();
        assert!(matches!(
            app.session_commands().create_session(request).await,
            Err(pontia_core::Error::CapabilityUnavailable(_))
        ));
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    use pontia_agent_control::client_contract::ClientConfiguration;
    let configuration = CodexConfiguration;
    let mut restricted = configuration
        .restricted_execution_policy("default")
        .unwrap();
    assert_eq!(restricted.tools, None);
    assert_eq!(restricted.disabled_tools, vec!["spawn_agent"]);
    assert!(!restricted.disable_mcp);
    assert!(!restricted.yolo);
    configuration
        .validate_execution_policy("default", &restricted)
        .unwrap();
    restricted.yolo = true;
    configuration
        .validate_execution_policy("default", &restricted)
        .unwrap();
}

#[tokio::test]
async fn codex_entry_settings_control_session_admission() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite(&format!("sqlite://{}", root.path().join("db").display()))
        .await
        .unwrap();
    run_migrations(&pool).await.unwrap();
    pontia_agent_control::workspaces::upsert_canonical_workspace(
        &pool,
        root.path().canonicalize().unwrap().to_str().unwrap(),
        None,
    )
    .await
    .unwrap();
    let mut clients = ClientRegistry::default();
    clients.register(pontia_client_codex::registration());
    let app = AgentControl::builder(pool.clone(), root.path().into())
        .clients(clients.clone())
        .build();
    let service = AgentClientService::new(pool.clone(), clients);
    let vars = std::collections::HashMap::new();
    let context = pontia_agent_control::client_contract::DetectionContext {
        vars: &vars,
        pontia_home: root.path(),
    };
    let entries = service.list(&context).await.unwrap();
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
    service.save("codex", "default", true).await.unwrap();
    create().await.unwrap();
    service.save("codex", "default", false).await.unwrap();
    assert!(matches!(
        create().await,
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
    service.save("codex", "default", true).await.unwrap();
    create().await.unwrap();
    service.delete("codex", "default").await.unwrap();
    assert!(matches!(
        create().await,
        Err(pontia_core::Error::CapabilityUnavailable(_))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM session_runtimes")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    pool.close().await;
}

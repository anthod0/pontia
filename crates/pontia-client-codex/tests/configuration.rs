use pontia_agent_control::{
    AgentControl, agent_clients::AgentClientService, clients::ClientRegistry,
};
use pontia_storage_sqlite::{connect_sqlite, run_migrations};
use serde_json::json;

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

#[tokio::test]
#[ignore = "requires the native codex executable"]
async fn trusts_only_the_selected_directory_through_the_background_server() {
    use std::{fs, path::Path, process::Command};

    const SELECTED: &str = "PONTIA_CODEX_TEST_TRUST_DIRECTORY";
    // Run the public operation with isolated process-local Pontia and Codex homes.
    if let Some(selected) = std::env::var_os(SELECTED) {
        let fixture = std::env::current_dir().unwrap();
        for (var, subdir) in [("PONTIA_HOME", "pontia"), ("CODEX_HOME", "codex")] {
            assert_eq!(std::env::var_os(var).unwrap(), fixture.join(subdir));
        }
        let db = connect_sqlite("sqlite::memory:").await.unwrap();
        run_migrations(&db).await.unwrap();
        let mut registry = ClientRegistry::default();
        registry.register(pontia_client_codex::registration());
        let clients = AgentClientService::new(db, registry);
        clients.save("codex", "default", true).await.unwrap();
        clients.trust_directory(Path::new(&selected)).await.unwrap();
        return;
    }

    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().canonicalize().unwrap();
    let pontia = root.join("pontia");
    let codex = root.join("codex");
    let repo = root.join("repo");
    let selected = repo.join(r#"child.with "quotes" and \ backslash"#);
    fs::create_dir(&pontia).unwrap();
    fs::create_dir(&codex).unwrap();
    fs::create_dir_all(&selected).unwrap();
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .unwrap()
            .success()
    );
    let config = codex.join("config.toml");
    let original = "# saved decision\n[projects.\"/other\"]\ntrust_level = \"untrusted\" # keep\n";
    fs::write(&config, original).unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "trusts_only_the_selected_directory_through_the_background_server",
        ])
        .current_dir(&root)
        .env("PONTIA_HOME", &pontia)
        .env("CODEX_HOME", &codex)
        .env(SELECTED, &selected)
        .status()
        .unwrap();
    let server = pontia_client_codex::runtime::background_server(&pontia)
        .await
        .unwrap();
    for process in &server.owned {
        pontia_runtime::GenericRuntimeManager
            .terminate_process(process)
            .unwrap();
    }
    assert!(result.success());
    assert_eq!(server.owned.len(), 1);
    assert!(
        !codex
            .join("app-server-control/app-server-control.sock")
            .exists()
    );
    let text = fs::read_to_string(config).unwrap();
    assert!(text.contains(original));
    let saved: toml::Table = toml::from_str(&text).unwrap();
    let projects = saved["projects"].as_table().unwrap();
    assert_eq!(projects.len(), 2);
    assert_eq!(
        projects[selected.to_str().unwrap()]["trust_level"].as_str(),
        Some("trusted")
    );
}

use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_client_acp::{AcpAgents, configuration::AcpConfiguration};
use serde_json::json;

#[test]
fn builtin_launch_and_management_settings() {
    let configuration = AcpConfiguration::new(AcpAgents::builtin());
    assert_eq!(configuration.keys(), ["antigravity", "grok", "kiro"]);
    assert_eq!(
        configuration.resolve("grok").unwrap(),
        json!({"name":"Grok Build","command":"grok","args":["agent","stdio"],"env":{}})
    );
    assert_eq!(
        configuration.resolve("antigravity").unwrap(),
        json!({"name":"Google Antigravity","command":"agy","args":["acp"],"env":{}})
    );
    assert_eq!(
        configuration.resolve("kiro").unwrap(),
        json!({"name":"Kiro","command":"kiro-cli","args":["acp","--agent-engine=v3"],"env":{}})
    );
    assert!(configuration.resolve("unknown").is_err());
    for key in [".", "..", ""] {
        assert!(configuration.key(&json!({"acp_agent_id":key})).is_err());
        assert!(configuration.resolve(key).is_err());
    }
}

#[test]
fn acp_validates_the_selected_definition() {
    let agents = AcpAgents::builtin();
    let mut other = agents.get("grok").unwrap().clone();
    other.restricted_execution = None;
    let configuration = AcpConfiguration::new(AcpAgents::new([
        ("restricted".into(), agents.get("grok").unwrap().clone()),
        ("plain".into(), other),
    ]));
    use pontia_core::SessionMode::{Default, Restricted, Yolo};
    for mode in [Default, Restricted, Yolo] {
        assert!(configuration.supports_mode("restricted", mode));
        assert_eq!(configuration.supports_mode("plain", mode), mode == Default);
    }
    let builtin = AcpConfiguration::new(AcpAgents::builtin());
    for key in ["kiro", "antigravity"] {
        assert!(builtin.supports_mode(key, Restricted));
        assert!(!builtin.supports_mode(key, Yolo));
    }
}

#[tokio::test]
async fn directory_trust_runs_grok_only_for_the_selected_root() {
    use pontia_agent_control::{agent_clients::AgentClientService, clients::ClientRegistry};
    use std::{fs, os::unix::fs::PermissionsExt, sync::Arc};

    let root = tempfile::tempdir().unwrap();
    let selected = root.path().join("project with spaces");
    let child = selected.join("child");
    fs::create_dir_all(&child).unwrap();
    let executable = root.path().join("grok");
    fs::write(
        &executable,
        r#"#!/bin/sh
case "$*" in
    'inspect --json') cat inspection.json ;;
    '--trust inspect --json') echo granted > decision ;;
    *) exit 1 ;;
esac
"#,
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();

    let builtin = AcpAgents::builtin();
    let agents = AcpAgents::new(builtin.ids().map(|key| {
        let mut agent = builtin.get(key).unwrap().clone();
        agent.launch.command = executable.display().to_string();
        (key.into(), agent)
    }));
    let mut entry = pontia_client_acp::registration();
    entry.configuration = Some(Arc::new(AcpConfiguration::new(agents)));
    let mut registry = ClientRegistry::default();
    registry.register(entry);
    let pool = pontia_storage_sqlite::connect_sqlite("sqlite::memory:")
        .await
        .unwrap();
    pontia_storage_sqlite::run_migrations(&pool).await.unwrap();
    let clients = AgentClientService::new(pool, registry);
    for key in ["kiro", "antigravity"] {
        clients.save("acp", key, true).await.unwrap();
    }
    clients.save("acp", "grok", false).await.unwrap();
    clients.trust_directory(&selected).await.unwrap();
    clients.save("acp", "grok", true).await.unwrap();

    let report = json!({"projectRoot": selected.canonicalize().unwrap()}).to_string();
    fs::write(child.join("inspection.json"), &report).unwrap();
    let error = clients
        .trust_directory(&child)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains(selected.to_str().unwrap()));
    assert!(!child.join("decision").exists());

    fs::write(selected.join("inspection.json"), "{}").unwrap();
    assert!(clients.trust_directory(&selected).await.is_err());
    assert!(!selected.join("decision").exists());
    fs::write(selected.join("inspection.json"), report).unwrap();
    clients.trust_directory(&selected.join(".")).await.unwrap();
    assert_eq!(
        fs::read_to_string(selected.join("decision")).unwrap(),
        "granted\n"
    );
    assert!(clients.trust_directory(&executable).await.is_err());
}

use pontia_config::{AppConfig, ClientConfigDefaults};
use std::collections::HashMap;

const DEFAULTS: ClientConfigDefaults = ClientConfigDefaults {
    default_client_type: "pi",
    runtime_commands: &[],
};

fn load(contents: &str) -> pontia_core::Result<AppConfig> {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("config.toml"), contents).unwrap();
    AppConfig::from_vars(
        &HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]),
        &DEFAULTS,
    )
}

#[test]
fn local_agents_preserve_literal_arguments_and_environment_overrides() {
    let config = load(
        r#"
[acp.agents.company]
name = "Company Agent"
command = "agent"
args = ["--acp", "argument with spaces", "${LITERAL}"]
[acp.agents.company.env]
LOG_LEVEL = "info"
TOKEN = "${FROM_ENV}"
[acp.agents.minimal]
command = "/opt/local-agent"
"#,
    )
    .unwrap();
    let agent = &config.acp.agents["company"];
    assert_eq!(agent.command, "agent");
    assert_eq!(agent.args, ["--acp", "argument with spaces", "${LITERAL}"]);
    assert_eq!(agent.env["TOKEN"], "${FROM_ENV}");
    assert!(config.acp.agents["minimal"].args.is_empty());
    assert!(config.acp.agents["minimal"].env.is_empty());
    assert!(config.acp.agents.contains_key("grok"));
    assert!(config.acp.agents.contains_key("kimi"));
    assert!(config.acp.agents.contains_key("opencode"));
}

#[test]
fn default_agents_use_local_commands_without_package_installation() {
    let root = tempfile::tempdir().unwrap();
    let without_file = AppConfig::from_vars(
        &HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]),
        &DEFAULTS,
    )
    .unwrap();
    for config in [without_file, load("").unwrap(), load("[acp]").unwrap()] {
        assert_eq!(config.acp.agents.len(), 3);
        for (id, command, args) in [
            ("grok", "grok", vec!["agent", "stdio"]),
            ("kimi", "kimi", vec!["acp"]),
            ("opencode", "opencode", vec!["acp"]),
        ] {
            let agent = &config.acp.agents[id];
            assert_eq!(agent.command, command);
            assert_eq!(agent.args, args);
            assert!(agent.registry_id.is_none());
            assert!(agent.env.is_empty());
        }
    }
}

#[test]
fn same_name_local_agent_replaces_the_whole_default_entry_only() {
    let config = load(
        r#"
[acp.agents.grok]
command = "/opt/grok"
[acp.agents.grok.env]
TOKEN = "${LITERAL}"
"#,
    )
    .unwrap();
    assert_eq!(config.acp.agents.len(), 3);
    let agent = &config.acp.agents["grok"];
    assert_eq!(agent.command, "/opt/grok");
    assert!(agent.args.is_empty());
    assert!(agent.name.is_none());
    assert!(agent.registry_id.is_none());
    assert_eq!(agent.env["TOKEN"], "${LITERAL}");
    assert_eq!(config.acp.agents["kimi"].args, ["acp"]);
    assert_eq!(config.acp.agents["opencode"].args, ["acp"]);
}

#[test]
fn same_name_registry_agent_replaces_the_local_default_source() {
    let config = load(
        r#"
[acp.agents.grok]
registry_id = "grok-build"
"#,
    )
    .unwrap();
    assert_eq!(config.acp.agents.len(), 3);
    let agent = &config.acp.agents["grok"];
    assert_eq!(agent.registry_id.as_deref(), Some("grok-build"));
    assert!(agent.command.is_empty());
    assert!(agent.args.is_empty());
    assert_eq!(config.acp.agents["kimi"].command, "kimi");
    assert_eq!(config.acp.agents["opencode"].command, "opencode");
}

#[test]
fn registry_agents_keep_the_configuration_identity_and_literal_environment() {
    let config = load(
        r#"
[acp.agents.company-gemini]
registry_id = "gemini"
[acp.agents.company-gemini.env]
TOKEN = "${UNEXPANDED}"
"#,
    )
    .unwrap();
    let agent = &config.acp.agents["company-gemini"];
    assert_eq!(agent.registry_id.as_deref(), Some("gemini"));
    assert!(agent.command.is_empty());
    assert!(agent.args.is_empty());
    assert_eq!(agent.env["TOKEN"], "${UNEXPANDED}");
}

#[test]
fn rejects_invalid_sources_and_protocol_settings_instead_of_silently_ignoring_them() {
    for contents in [
        "[acp.agents.agent]\nregistry_id=''",
        "[acp.agents.agent]\nregistry_id='gemini'\nargs=[]",
        "[acp.agents.agent]\nregistry_id='gemini'\nargs=['--override']",
        "[acp.agents.agent]\ncommand='agent'\nregistry_id='gemini'",
        "[acp.agents.agent]\ncommand='agent'\ntransport='http'",
        "[acp.agents.agent]\ncommand='agent'\ncapabilities={timeline=true}",
        "[acp.agents.agent]\ncommand='agent'\ncwd='/fixed'",
        "[acp.agents.agent]\ncommand=''",
        "[acp.agents.\"\"]\ncommand='agent'",
        "[acp.agents.agent]\nargs=['--acp']",
    ] {
        assert!(load(contents).is_err(), "accepted {contents}");
    }
}

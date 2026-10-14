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
    assert!(load("").unwrap().acp.agents.is_empty());
}

#[test]
fn rejects_unsupported_sources_and_protocol_settings_instead_of_silently_ignoring_them() {
    for contents in [
        "[acp.agents.agent]\nregistry_id='gemini'",
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

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
fn obsolete_acp_table_is_ignored_when_loading_configuration() {
    let plain = load("").unwrap();
    let config = load(
        "[acp.agents.x]
command = 'ignored'",
    )
    .unwrap();
    assert_eq!(config.client_configuration, plain.client_configuration);
    assert_eq!(config.default_client_type, plain.default_client_type);
}

use pontia_config::AppConfig;
use std::collections::HashMap;

fn load(contents: &str) -> pontia_core::Result<AppConfig> {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("config.toml"), contents).unwrap();
    AppConfig::from_vars(
        &HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]),
        &[],
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
}

use pontia_config::{AppConfig, ClientConfigDefaults};
use serde_json::json;
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
fn file_layer_preserves_partial_fields_and_literal_arguments() {
    let config = load(
        r#"
[acp.agents.company]
name = "Company Agent"
command = "agent"
args = ["--acp", "argument with spaces", "${LITERAL}"]
[acp.agents.company.env]
TOKEN = "${FROM_ENV}"
[acp.agents.kimi]
name = "Renamed Kimi"
"#,
    )
    .unwrap();
    let file = config.client_configuration.acp_file;
    assert_eq!(
        file["company"]["args"],
        json!(["--acp", "argument with spaces", "${LITERAL}"])
    );
    assert_eq!(file["company"]["env"]["TOKEN"], "${FROM_ENV}");
    assert_eq!(file["kimi"], json!({"name":"Renamed Kimi"}));
}

#[test]
fn missing_file_fields_are_not_materialized_as_defaults() {
    for contents in ["", "[acp]", "[acp.agents]"] {
        assert!(
            load(contents)
                .unwrap()
                .client_configuration
                .acp_file
                .is_empty()
        );
    }
}

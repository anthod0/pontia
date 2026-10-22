use pontia_application::client_contract::ClientConfiguration;
use pontia_config::ConfigurationLayers;
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub struct CodexConfiguration(pub ConfigurationLayers);

impl CodexConfiguration {
    pub fn new(file: Value) -> Self {
        Self(ConfigurationLayers {
            defaults: json!({"tui_command":"codex"}),
            file,
            environment: json!({}),
        })
    }
}

impl ClientConfiguration for CodexConfiguration {
    fn keys(&self) -> Vec<String> {
        vec!["default".into()]
    }
    fn presentation(&self, _key: &str, _effective: &Value) -> (String, String) {
        ("Codex".into(), "codex".into())
    }
    fn key(&self, _metadata: &Value) -> Result<String> {
        Ok("default".into())
    }
    fn merged(&self, _key: &str, overrides: &Value) -> Value {
        self.0.resolve(overrides)
    }
    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value> {
        if key != "default" {
            return Err(Error::Domain(
                "Codex requires the default client key".into(),
            ));
        }
        let value = self.merged(key, overrides);
        command(&value)?;
        Ok(value)
    }
}

pub(crate) fn command(value: &Value) -> Result<String> {
    let config: pontia_config::RuntimeClientConfig = serde_json::from_value(value.clone())?;
    config
        .tui_command
        .filter(|command| !command.trim().is_empty())
        .ok_or_else(|| Error::Domain("Codex tui_command must be nonempty".into()))
}

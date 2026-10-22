use std::collections::BTreeMap;

use pontia_application::client_contract::ClientConfiguration;
mod launch;
pub use launch::{AcpAgentConfig, AcpConfig};
use pontia_config::ConfigurationLayers;
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub struct AcpConfiguration {
    pub(crate) defaults: BTreeMap<String, Value>,
    pub(crate) file: BTreeMap<String, Value>,
}

pub(crate) fn value(config: &AcpAgentConfig) -> Value {
    json!({
        "name": config.name,
        "registry_id": config.registry_id,
        "command": if config.registry_id.is_some() { None } else { Some(&config.command) },
        "args": if config.registry_id.is_some() { None } else { Some(&config.args) },
        "env": config.env,
    })
}

impl AcpConfiguration {
    pub fn new(file: BTreeMap<String, Value>) -> Self {
        Self::with_defaults(AcpConfig::default(), file)
    }

    pub(crate) fn with_defaults(defaults: AcpConfig, file: BTreeMap<String, Value>) -> Self {
        Self {
            defaults: defaults
                .agents
                .into_iter()
                .map(|(id, config)| (id, value(&config)))
                .collect(),
            file,
        }
    }
}

impl ClientConfiguration for AcpConfiguration {
    fn key(&self, metadata: &Value) -> Result<String> {
        metadata["acp_agent_id"]
            .as_str()
            .filter(|key| !key.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| Error::Domain("metadata.acp_agent_id must be a nonempty string".into()))
    }

    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value> {
        if key.is_empty() {
            return Err(Error::Domain("ACP Agent ID must be nonempty".into()));
        }
        let value = ConfigurationLayers {
            defaults: self.defaults.get(key).cloned().unwrap_or_else(|| json!({})),
            file: self.file.get(key).cloned().unwrap_or_else(|| json!({})),
            environment: json!({}),
        }
        .resolve(overrides);
        let _: AcpAgentConfig = serde_json::from_value(value.clone())?;
        Ok(value)
    }
}

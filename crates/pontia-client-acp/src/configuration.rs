use std::collections::BTreeMap;

use pontia_agent_control::client_contract::ClientConfiguration;
mod launch;
pub use launch::{AcpAgentConfig, AcpConfig};
use pontia_config::ConfigurationLayers;
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub struct AcpConfiguration {
    pub(crate) defaults: BTreeMap<String, Value>,
    pub(crate) file: BTreeMap<String, Value>,
}

fn validate_agent_id(id: &str) -> Result<()> {
    if id.is_empty() || matches!(id, "." | "..") {
        return Err(Error::Domain(
            "ACP Agent ID must be nonempty and cannot be . or ..".into(),
        ));
    }
    Ok(())
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
    fn detect(
        &self,
        effective: &Value,
        context: &pontia_agent_control::client_contract::DetectionContext<'_>,
    ) -> pontia_agent_control::client_contract::ClientDetection {
        use pontia_agent_control::client_contract::{ClientDetection, DetectionContext};
        let config: AcpAgentConfig = match serde_json::from_value(effective.clone()) {
            Ok(config) => config,
            Err(error) => return ClientDetection::unknown(error.to_string()),
        };
        if config.registry_id.is_some() {
            return ClientDetection::unknown(
                "Registry agents are resolved and downloaded when started",
            );
        }
        let mut vars = context.vars.clone();
        vars.extend(config.env);
        DetectionContext {
            vars: &vars,
            pontia_home: context.pontia_home,
        }
        .command(&config.command)
    }
    fn keys(&self) -> Vec<String> {
        self.defaults
            .keys()
            .chain(self.file.keys())
            .cloned()
            .collect()
    }
    fn presentation(&self, key: &str, effective: &Value) -> (String, String) {
        (
            effective["name"]
                .as_str()
                .filter(|name| !name.is_empty())
                .unwrap_or(key)
                .into(),
            key.into(),
        )
    }
    fn key(&self, metadata: &Value) -> Result<String> {
        let key = metadata["acp_agent_id"]
            .as_str()
            .ok_or_else(|| Error::Domain("metadata.acp_agent_id must be a string".into()))?;
        validate_agent_id(key)?;
        Ok(key.into())
    }

    fn merged(&self, key: &str, overrides: &Value) -> Value {
        ConfigurationLayers {
            defaults: self.defaults.get(key).cloned().unwrap_or_else(|| json!({})),
            file: self.file.get(key).cloned().unwrap_or_else(|| json!({})),
            environment: json!({}),
        }
        .resolve(overrides)
    }

    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value> {
        validate_agent_id(key)?;
        let value = self.merged(key, overrides);
        let _: AcpAgentConfig = serde_json::from_value(value.clone())?;
        Ok(value)
    }
}

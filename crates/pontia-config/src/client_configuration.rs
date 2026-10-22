use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

/// File and process-environment snapshots captured when pontiad loads configuration.
/// Client integrations own defaults and final validation; database settings are read per creation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientConfigurationSources {
    pub runtime_file: BTreeMap<String, Value>,
    pub runtime_environment: BTreeMap<String, Value>,
    pub acp_file: BTreeMap<String, Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AcpFileConfig {
    #[serde(default)]
    pub agents: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct ConfigurationLayers {
    pub defaults: Value,
    pub file: Value,
    pub environment: Value,
}

impl ConfigurationLayers {
    pub fn resolve(&self, overrides: &Value) -> Value {
        let mut result = self.defaults.clone();
        for layer in [&self.file, overrides, &self.environment] {
            merge(&mut result, layer);
        }
        result
    }
}

/// Null remains a tombstone until final client validation. It is never filled by defaults again.
pub fn merge(target: &mut Value, overlay: &Value) {
    if let Value::Object(fields) = overlay {
        if !target.is_object() {
            *target = json!({});
        }
        for (key, value) in fields {
            merge(&mut target[key], value);
        }
    } else {
        *target = overlay.clone();
    }
}

use std::collections::BTreeMap;

use serde_json::{Value, json};

/// File and process-environment snapshots captured when pontiad loads configuration.
/// Client integrations own defaults and final validation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientConfigurationSources {
    pub runtime_file: BTreeMap<String, Value>,
    pub runtime_environment: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct ConfigurationLayers {
    pub defaults: Value,
    pub file: Value,
    pub environment: Value,
}

impl ConfigurationLayers {
    pub fn resolve(&self) -> Value {
        let mut result = self.defaults.clone();
        for layer in [&self.file, &self.environment] {
            merge(&mut result, layer);
        }
        result
    }
}

fn merge(target: &mut Value, overlay: &Value) {
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

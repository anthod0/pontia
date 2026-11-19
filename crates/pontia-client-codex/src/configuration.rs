use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_config::ConfigurationLayers;
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub struct CodexConfiguration(pub ConfigurationLayers);

impl CodexConfiguration {
    pub fn new(file: Value) -> Self {
        Self(ConfigurationLayers {
            defaults: json!({}),
            file,
            environment: json!({}),
        })
    }
}

impl ClientConfiguration for CodexConfiguration {
    fn validate_execution_policy(
        &self,
        policy: &pontia_core::execution_policy::ExecutionPolicy,
    ) -> Result<()> {
        if policy.tools.is_some() {
            return Err(Error::CapabilityUnavailable(
                "Codex cannot restrict its tools to the requested allowlist".into(),
            ));
        }
        if !policy.disabled_tools.is_empty() {
            return Err(Error::CapabilityUnavailable(
                "Codex cannot enforce a native tool denylist".into(),
            ));
        }
        if policy.disable_mcp {
            return Err(Error::CapabilityUnavailable(
                "Codex cannot disable MCP through execution policy".into(),
            ));
        }
        Ok(())
    }

    fn supports_startup_model(&self) -> bool {
        true
    }

    fn list_models<'a>(
        &'a self,
        _: &'a Value,
        directory: &'a std::path::Path,
    ) -> pontia_agent_control::client_contract::ClientOperation<
        'a,
        pontia_agent_control::agent_clients::AgentClientModels,
    > {
        Box::pin(crate::CodexService::available_models(directory))
    }

    fn detect(
        &self,
        _: &Value,
        context: &pontia_agent_control::client_contract::DetectionContext<'_>,
    ) -> pontia_agent_control::client_contract::ClientDetection {
        crate::setup::detect(context)
    }
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
        Ok(self.merged(key, overrides))
    }
}

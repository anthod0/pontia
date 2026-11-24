use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub struct CodexConfiguration;

impl ClientConfiguration for CodexConfiguration {
    fn supports_mode(&self, _key: &str, _mode: pontia_core::SessionMode) -> bool {
        true
    }

    fn supports_startup_model(&self) -> bool {
        true
    }

    fn supports_startup_reasoning_effort(&self) -> bool {
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
    fn merged(&self, _key: &str) -> Value {
        json!({})
    }
    fn resolve(&self, key: &str) -> Result<Value> {
        if key != "default" {
            return Err(Error::Domain(
                "Codex requires the default client key".into(),
            ));
        }
        Ok(self.merged(key))
    }
}

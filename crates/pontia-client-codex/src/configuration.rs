use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub(crate) const SPAWN_AGENT: &str = "spawn_agent";

pub struct CodexConfiguration;

impl ClientConfiguration for CodexConfiguration {
    fn restricted_execution_policy(
        &self,
        _key: &str,
    ) -> Option<pontia_core::execution_policy::ExecutionPolicy> {
        Some(pontia_core::execution_policy::ExecutionPolicy {
            disabled_tools: vec![SPAWN_AGENT.into()],
            yolo: false,
            ..Default::default()
        })
    }

    fn validate_execution_policy(
        &self,
        _key: &str,
        policy: &pontia_core::execution_policy::ExecutionPolicy,
    ) -> Result<()> {
        if policy.tools.is_some() {
            return Err(Error::CapabilityUnavailable(
                "Codex cannot restrict its tools to the requested allowlist".into(),
            ));
        }
        if let Some(name) = policy
            .disabled_tools
            .iter()
            .find(|name| name.as_str() != SPAWN_AGENT)
        {
            return Err(Error::CapabilityUnavailable(format!(
                "Codex cannot disable tool {name}"
            )));
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

use crate::runtime::{CodexRuntime, CodexServerTarget};
use pontia_agent_control::client_contract::{
    ClientConfiguration, ClientOperation, DetectionContext,
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::path::Path;

pub struct CodexConfiguration;

impl ClientConfiguration for CodexConfiguration {
    fn trust_directory<'a>(&'a self, _: &'a str, directory: &'a Path) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let directory = directory.to_str().ok_or_else(|| {
                Error::Domain("Codex directory trust requires a UTF-8 path".into())
            })?;
            let home = pontia_config::pontia_home(&std::env::vars().collect())?;
            let runtime = CodexRuntime::ensure(&home, CodexServerTarget::Background).await?;
            let connection = runtime.connection().await?;
            let key = directory.replace('\\', "\\\\").replace('"', "\\\"");
            // Codex preserves unrelated settings and comments on an individual write.
            // Concurrent config.toml writes by separate Codex processes can lose edits,
            // even with expectedVersion.
            let response = connection
                .call(
                    "config/value/write",
                    json!({
                        "keyPath": format!("projects.\"{key}\".trust_level"),
                        "value": "trusted",
                        "mergeStrategy": "replace",
                    }),
                )
                .await?;
            match response["status"].as_str() {
                Some("ok") => Ok(()),
                Some("okOverridden") => Err(Error::Domain(format!(
                    "Codex directory trust is overridden for {directory}"
                ))),
                _ => Err(Error::Domain(
                    "Codex did not confirm directory trust".into(),
                )),
            }
        })
    }

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

    fn probe_version<'a>(
        &'a self,
        _key: &'a str,
        context: &'a DetectionContext<'_>,
    ) -> ClientOperation<'a, Option<String>> {
        Box::pin(async move {
            let Ok(path) = crate::setup::resolve_executable(context.search_path()) else {
                return Ok(None);
            };
            Ok(context.cli_version(&path, "codex-cli ").await)
        })
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

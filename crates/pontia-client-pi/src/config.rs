use pontia_agent_control::client_contract::{
    ClientConfiguration, ClientOperation, DetectionContext,
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub struct PiConfiguration;

impl ClientConfiguration for PiConfiguration {
    fn supports_mode(&self, _key: &str, _mode: pontia_core::SessionMode) -> bool {
        true
    }

    fn private_directory_files(
        &self,
    ) -> &'static [pontia_agent_control::client_contract::LaunchFile] {
        &[pontia_agent_control::client_contract::LaunchFile {
            path: ".pi/settings.json",
            contents: r#"{"steeringMode":"all"}"#,
        }]
    }

    fn supports_startup_reasoning_effort(&self) -> bool {
        true
    }

    fn supports_startup_model(&self) -> bool {
        true
    }

    fn list_models<'a>(
        &'a self,
        _effective: &'a Value,
        directory: &'a std::path::Path,
    ) -> pontia_agent_control::client_contract::ClientOperation<
        'a,
        pontia_agent_control::agent_clients::AgentClientModels,
    > {
        Box::pin(async move { crate::models::list(directory).await })
    }

    fn probe_version<'a>(
        &'a self,
        _key: &'a str,
        context: &'a DetectionContext<'_>,
    ) -> ClientOperation<'a, Option<String>> {
        Box::pin(async move {
            let Ok(Some(path)) = context.executable("pi") else {
                return Ok(None);
            };
            Ok(context.cli_version(&path, "").await)
        })
    }

    fn detect(
        &self,
        _effective: &Value,
        context: &pontia_agent_control::client_contract::DetectionContext<'_>,
    ) -> pontia_agent_control::client_contract::ClientDetection {
        context.command("pi")
    }
    fn keys(&self) -> Vec<String> {
        vec!["default".into()]
    }
    fn presentation(&self, _key: &str, _effective: &Value) -> (String, String) {
        ("Pi".into(), "pi".into())
    }
    fn key(&self, _metadata: &Value) -> Result<String> {
        Ok("default".into())
    }

    fn merged(&self, _key: &str) -> Value {
        json!({})
    }
    fn resolve(&self, key: &str) -> Result<Value> {
        if key != "default" {
            return Err(Error::Domain("Pi requires the default client key".into()));
        }
        Ok(self.merged(key))
    }
}

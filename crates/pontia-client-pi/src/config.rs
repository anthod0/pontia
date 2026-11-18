use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_config::{ClientConfigDefaults, ConfigurationLayers, RuntimeCommandOverride};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub struct PiConfiguration(pub ConfigurationLayers);

impl PiConfiguration {
    pub fn new(file: Value, mut environment: Value) -> Self {
        // An empty environment setting retains its existing meaning: use the built-in command.
        if environment.get("tui_command").is_some_and(Value::is_null) {
            environment["tui_command"] = json!("pi");
        }
        Self(ConfigurationLayers {
            defaults: json!({"tui_command":"pi"}),
            file,
            environment,
        })
    }
}

impl ClientConfiguration for PiConfiguration {
    fn supports_startup_model(&self) -> bool {
        true
    }

    fn list_models<'a>(
        &'a self,
        effective: &'a Value,
        directory: &'a std::path::Path,
    ) -> pontia_agent_control::client_contract::ClientOperation<
        'a,
        pontia_agent_control::agent_clients::AgentClientModels,
    > {
        Box::pin(async move { crate::models::list(&command(effective)?, directory).await })
    }

    fn detect(
        &self,
        effective: &Value,
        context: &pontia_agent_control::client_contract::DetectionContext<'_>,
    ) -> pontia_agent_control::client_contract::ClientDetection {
        use pontia_agent_control::client_contract::ClientDetection;
        let command = match command(effective) {
            Ok(command) => command,
            Err(error) => return ClientDetection::unknown(error.to_string()),
        };
        if command
            .chars()
            .any(|c| c.is_whitespace() || "'\"$`;|&()<>{}*?~\\".contains(c))
        {
            return ClientDetection::unknown(
                "This shell command must be checked in its launch environment",
            );
        }
        context.command(&command)
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

    fn merged(&self, _key: &str, overrides: &Value) -> Value {
        self.0.resolve(overrides)
    }
    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value> {
        if key != "default" {
            return Err(Error::Domain("Pi requires the default client key".into()));
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
        .ok_or_else(|| Error::Domain("Pi tui_command must be nonempty".into()))
}

pub const DEFAULTS: ClientConfigDefaults = ClientConfigDefaults {
    default_client_type: "pi",
    runtime_commands: &[RuntimeCommandOverride {
        client_type: "pi",
        environment_variable: "PONTIA_PI_TUI_COMMAND",
    }],
};

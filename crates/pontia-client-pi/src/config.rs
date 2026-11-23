use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_config::{ClientConfigDefaults, ConfigurationLayers, RuntimeCommandOverride};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub struct PiConfiguration(pub ConfigurationLayers);

impl PiConfiguration {
    pub fn new(file: Value, mut environment: Value) -> Self {
        // An empty PONTIA_PI_TUI_COMMAND selects the built-in command.
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
    fn restricted_execution_policy(
        &self,
        _key: &str,
    ) -> Option<pontia_core::execution_policy::ExecutionPolicy> {
        Some(pontia_core::execution_policy::ExecutionPolicy {
            disabled_tools: ["edit", "write"].into_iter().map(str::to_owned).collect(),
            disable_mcp: true,
            yolo: false,
            ..Default::default()
        })
    }

    fn validate_execution_policy(
        &self,
        _key: &str,
        policy: &pontia_core::execution_policy::ExecutionPolicy,
    ) -> Result<()> {
        for tool in policy.tools.iter().flatten().chain(&policy.disabled_tools) {
            if tool.trim().is_empty() || tool.contains(',') || tool.starts_with(['+', '-']) {
                return Err(Error::Domain(
                    "Pi tool entries must be nonempty native names or patterns without commas or +/- prefixes".into(),
                ));
            }
        }
        Ok(())
    }

    fn supports_startup_reasoning_effort(&self) -> bool {
        true
    }

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

    fn merged(&self, _key: &str) -> Value {
        self.0.resolve()
    }
    fn resolve(&self, key: &str) -> Result<Value> {
        if key != "default" {
            return Err(Error::Domain("Pi requires the default client key".into()));
        }
        let value = self.merged(key);
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

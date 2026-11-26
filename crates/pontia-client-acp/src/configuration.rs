use crate::{AcpAgentDefinition, AcpAgents, AcpLaunch};
use pontia_agent_control::client_contract::{
    ClientConfiguration, ClientOperation, DetectionContext,
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::path::Path;

pub struct AcpConfiguration {
    pub(crate) agents: AcpAgents,
}

fn validate_agent_id(id: &str) -> Result<()> {
    if id.is_empty() || matches!(id, "." | "..") {
        return Err(Error::Domain(
            "ACP Agent ID must be nonempty and cannot be . or ..".into(),
        ));
    }
    Ok(())
}

impl AcpConfiguration {
    pub fn new(agents: AcpAgents) -> Self {
        Self { agents }
    }

    pub(crate) fn definition(&self, key: &str) -> Result<&AcpAgentDefinition> {
        validate_agent_id(key)?;
        self.agents
            .get(key)
            .ok_or_else(|| Error::Domain(format!("Unknown ACP Agent: {key}")))
    }
}

fn effective_config(agent: &AcpAgentDefinition) -> Value {
    json!({
        "name": agent.name,
        "command": agent.launch.command,
        "args": agent.launch.args,
        "env": agent.launch.env,
    })
}

impl ClientConfiguration for AcpConfiguration {
    fn trust_directory<'a>(&'a self, key: &'a str, directory: &'a Path) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            if key == "grok" {
                let launch =
                    crate::peer::resolve_command(&self.definition(key)?.launch, directory)?;
                trust_grok_directory(&launch, directory).await?;
            }
            Ok(())
        })
    }

    fn supports_mode(&self, key: &str, mode: pontia_core::SessionMode) -> bool {
        self.agents.get(key).is_some_and(|agent| {
            crate::restrictions::SessionPolicy::resolve(agent.restricted_execution, mode).is_ok()
        })
    }

    fn supports_startup_model(&self) -> bool {
        true
    }

    fn supports_startup_reasoning_effort(&self) -> bool {
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
        Box::pin(async move {
            let launch: AcpLaunch = serde_json::from_value(effective.clone())?;
            let cwd = std::path::absolute(directory)?;
            let config = crate::peer::resolve_command(&launch, &cwd)?;
            crate::models::probe::list(&config, &cwd).await
        })
    }

    fn probe_version<'a>(
        &'a self,
        key: &'a str,
        context: &'a DetectionContext<'_>,
    ) -> ClientOperation<'a, Option<String>> {
        Box::pin(async move {
            let prefix = match key {
                "grok" => "grok ",
                "kiro" => "kiro-cli ",
                "antigravity" => "",
                _ => return Ok(None),
            };
            let launch = &self.definition(key)?.launch;
            let mut vars = context.vars.clone();
            vars.extend(launch.env.clone());
            let context = DetectionContext {
                vars: &vars,
                pontia_home: context.pontia_home,
            };
            let Ok(Some(path)) = context
                .executable_with_path(&launch.command, launch.env.get("PATH").map(String::as_str))
            else {
                return Ok(None);
            };
            Ok(context.cli_version(&path, prefix).await)
        })
    }

    fn detect(
        &self,
        effective: &Value,
        context: &pontia_agent_control::client_contract::DetectionContext<'_>,
    ) -> pontia_agent_control::client_contract::ClientDetection {
        use pontia_agent_control::client_contract::ClientDetection;
        let config: AcpLaunch = match serde_json::from_value(effective.clone()) {
            Ok(config) => config,
            Err(error) => return ClientDetection::unknown(error.to_string()),
        };
        context.command_with_path(&config.command, config.env.get("PATH").map(String::as_str))
    }
    fn keys(&self) -> Vec<String> {
        self.agents.ids().map(str::to_owned).collect()
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

    fn merged(&self, key: &str) -> Value {
        self.agents
            .get(key)
            .map(effective_config)
            .unwrap_or_else(|| json!({}))
    }

    fn resolve(&self, key: &str) -> Result<Value> {
        self.definition(key).map(effective_config)
    }
}

async fn trust_grok_directory(launch: &AcpLaunch, directory: &Path) -> Result<()> {
    let inspect = async |args: &[&str]| -> Result<Vec<u8>> {
        let output = tokio::process::Command::new(&launch.command)
            .args(args)
            .current_dir(directory)
            .envs(&launch.env)
            .kill_on_drop(true)
            .output()
            .await?;
        if !output.status.success() {
            return Err(Error::Domain(format!(
                "Grok {} failed ({}): {}",
                args.join(" "),
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    };
    let report: Value = serde_json::from_slice(&inspect(&["inspect", "--json"]).await?)?;
    match report.get("projectRoot") {
        Some(Value::String(root)) if Path::new(root) != directory => {
            return Err(Error::Domain(format!(
                "Grok trusts the entire Git repository. Pass its root directory: {root}"
            )));
        }
        Some(Value::Null | Value::String(_)) => {}
        _ => {
            return Err(Error::Domain(
                "Grok inspect did not report its project root".into(),
            ));
        }
    }
    inspect(&["--trust", "inspect", "--json"]).await?;
    Ok(())
}

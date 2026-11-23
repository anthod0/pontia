use crate::{AcpAgentDefinition, AcpAgents, AcpLaunch};
use pontia_agent_control::client_contract::ClientConfiguration;
use pontia_core::{Error, Result};
use serde_json::{Value, json};

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
    fn restricted_execution_policy(
        &self,
        key: &str,
    ) -> Option<pontia_core::execution_policy::ExecutionPolicy> {
        self.agents
            .get(key)?
            .restricted_execution
            .map(|recipe| recipe.policy())
    }

    fn validate_execution_policy(
        &self,
        key: &str,
        policy: &pontia_core::execution_policy::ExecutionPolicy,
    ) -> Result<()> {
        let agent = self
            .agents
            .get(key)
            .ok_or_else(|| Error::Domain(format!("Unknown ACP Agent: {key}")))?;
        crate::restrictions::SessionPolicy::resolve(agent.restricted_execution, policy).map(|_| ())
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
        Box::pin(async move {
            let launch: AcpLaunch = serde_json::from_value(effective.clone())?;
            let cwd = std::path::absolute(directory)?;
            let config = crate::peer::resolve_command(&launch, &cwd)?;
            crate::models::probe::list(&config, &cwd).await
        })
    }

    fn detect(
        &self,
        effective: &Value,
        context: &pontia_agent_control::client_contract::DetectionContext<'_>,
    ) -> pontia_agent_control::client_contract::ClientDetection {
        use pontia_agent_control::client_contract::{ClientDetection, DetectionContext};
        let config: AcpLaunch = match serde_json::from_value(effective.clone()) {
            Ok(config) => config,
            Err(error) => return ClientDetection::unknown(error.to_string()),
        };
        let mut vars = context.vars.clone();
        vars.extend(config.env);
        DetectionContext {
            vars: &vars,
            pontia_home: context.pontia_home,
        }
        .command(&config.command)
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

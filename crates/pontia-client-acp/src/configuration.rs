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

    pub(crate) fn definition(&self, key: &str, overrides: &Value) -> Result<&AcpAgentDefinition> {
        validate_agent_id(key)?;
        let agent = self
            .agents
            .get(key)
            .ok_or_else(|| Error::Domain(format!("Unknown ACP Agent: {key}")))?;
        if !overrides
            .as_object()
            .is_some_and(|fields| fields.is_empty())
        {
            return Err(Error::Domain(
                "ACP Agents do not accept configuration overrides".into(),
            ));
        }
        Ok(agent)
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
        crate::capabilities::restricted_execution(self.agents.get(key))
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

    fn merged(&self, key: &str, _overrides: &Value) -> Value {
        self.agents
            .get(key)
            .map(effective_config)
            .unwrap_or_else(|| json!({}))
    }

    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value> {
        self.definition(key, overrides).map(effective_config)
    }
}

use std::collections::BTreeMap;

use pontia_core::{Error, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcpConfig {
    #[serde(default)]
    pub agents: BTreeMap<String, AcpAgentConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "AgentConfigInput")]
pub struct AcpAgentConfig {
    pub name: Option<String>,
    pub registry_id: Option<String>,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentConfigInput {
    name: Option<String>,
    registry_id: Option<String>,
    command: Option<String>,
    args: Option<Vec<String>>,
    #[serde(default)]
    env: BTreeMap<String, String>,
}

impl TryFrom<AgentConfigInput> for AcpAgentConfig {
    type Error = String;

    fn try_from(input: AgentConfigInput) -> std::result::Result<Self, Self::Error> {
        match (&input.registry_id, &input.command, &input.args) {
            (Some(id), None, None) if !id.is_empty() => {}
            (None, Some(command), _) if !command.is_empty() => {}
            _ => return Err("Select exactly one nonempty registry_id or command; Registry agents cannot override args".into()),
        }
        Ok(Self {
            name: input.name,
            registry_id: input.registry_id,
            command: input.command.unwrap_or_default(),
            args: input.args.unwrap_or_default(),
            env: input.env,
        })
    }
}

impl AcpConfig {
    pub fn validate(&self) -> Result<()> {
        for (id, agent) in &self.agents {
            let valid_source = match &agent.registry_id {
                Some(registry) => {
                    !registry.is_empty() && agent.command.is_empty() && agent.args.is_empty()
                }
                None => !agent.command.is_empty(),
            };
            if id.is_empty() || !valid_source {
                return Err(Error::InvalidConfig {
                    key: "acp.agents",
                    message: "Agent ID and exactly one valid launch source are required".into(),
                });
            }
        }
        Ok(())
    }
}

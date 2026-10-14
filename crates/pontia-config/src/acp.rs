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
#[serde(deny_unknown_fields)]
pub struct AcpAgentConfig {
    pub name: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

impl AcpConfig {
    pub fn validate(&self) -> Result<()> {
        for (id, agent) in &self.agents {
            if id.is_empty() || agent.command.is_empty() {
                return Err(Error::InvalidConfig {
                    key: "acp.agents",
                    message: "Agent ID and local command must not be empty".into(),
                });
            }
        }
        Ok(())
    }
}

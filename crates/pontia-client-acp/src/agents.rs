use std::{collections::BTreeMap, sync::Arc};

use pontia_core::{
    client_capabilities::AgentClientCapabilities, execution_policy::ExecutionPolicy,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcpLaunch {
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct AcpAgentDefinition {
    pub name: String,
    pub launch: AcpLaunch,
    pub capabilities: AgentClientCapabilities,
    pub restricted_execution: Option<ExecutionPolicy>,
}

#[derive(Clone)]
pub struct AcpAgents(Arc<BTreeMap<String, AcpAgentDefinition>>);

impl AcpAgents {
    pub fn builtin() -> Self {
        Self::new([(
            "grok".into(),
            AcpAgentDefinition {
                name: "Grok Build".into(),
                launch: AcpLaunch {
                    command: "grok".into(),
                    args: vec!["agent".into(), "stdio".into()],
                    env: BTreeMap::new(),
                },
                capabilities: crate::ACP_MAX_CAPABILITIES,
                restricted_execution: None,
            },
        )])
    }

    pub fn new(agents: impl IntoIterator<Item = (String, AcpAgentDefinition)>) -> Self {
        Self(Arc::new(agents.into_iter().collect()))
    }

    pub fn get(&self, key: &str) -> Option<&AcpAgentDefinition> {
        self.0.get(key)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
}

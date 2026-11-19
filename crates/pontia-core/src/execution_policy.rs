use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionTool {
    Read,
    Bash,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExecutionPolicy {
    /// None keeps the client's tool set; an empty list disables all tools.
    pub tools: Option<Vec<ExecutionTool>>,
    /// Bypass client approvals and client sandboxing.
    pub yolo: bool,
}

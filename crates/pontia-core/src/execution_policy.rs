use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ExecutionPolicy {
    /// Native client tool selection. None keeps the client's default selection.
    pub tools: Option<Vec<String>>,
    /// Native client tool names or deny rules. Denials take precedence over selection.
    pub disabled_tools: Vec<String>,
    /// Disable MCP connections and tools.
    pub disable_mcp: bool,
    /// Bypass client approvals and client sandboxing.
    pub yolo: bool,
}

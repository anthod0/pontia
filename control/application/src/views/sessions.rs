use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use pontia_agent_clients::ContextUsageCapability;
pub type SessionCapabilities = pontia_agent_clients::AgentClientCapabilities;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContextUsageView {
    pub used_tokens: Option<u64>,
    pub max_tokens: Option<u64>,
    pub remaining_tokens: Option<u64>,
    pub usage_ratio: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_tokens: Option<u64>,
    pub confidence: String,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionLineageView {
    pub relation_type: String,
    pub parent_session_id: String,
    pub forked_from_turn_id: Option<String>,
    pub forked_from_client_node_id: Option<String>,
    pub parent_client_session_key: Option<String>,
    pub child_client_session_key: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionView {
    pub session_id: String,
    pub client_type: String,
    pub title: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub state: String,
    pub current_turn_id: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace: Option<String>,
    pub pinned_at: Option<String>,
    pub archived_at: Option<String>,
    pub capabilities: SessionCapabilities,
    pub model: Option<String>,
    pub context_usage: Option<ContextUsageView>,
    pub lineage: Option<SessionLineageView>,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Value,
}

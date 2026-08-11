use serde::{Deserialize, Serialize};
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::sessions::SessionRow;

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

pub(crate) fn row_to_view(row: SessionRow) -> Result<SessionView> {
    let metadata: Value = serde_json::from_str(&row.metadata)?;
    let context_usage = metadata
        .get("context_usage")
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;
    let model = metadata
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok(SessionView {
        session_id: row.session_id,
        client_type: row.client_type,
        title: row.title,
        handle: row.handle,
        role: row.role,
        description: row.description,
        execution_profile_id: row.execution_profile_id,
        execution_profile_version: row.execution_profile_version,
        state: row.state,
        current_turn_id: row.current_turn_id,
        workspace_id: row.workspace_id,
        workspace: row.workspace_ref,
        pinned_at: row.pinned_at,
        archived_at: row.archived_at,
        capabilities: SessionCapabilities::default(),
        model,
        context_usage,
        lineage: None,
        created_at: row.created_at,
        updated_at: row.updated_at,
        metadata,
    })
}

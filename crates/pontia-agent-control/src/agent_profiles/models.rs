use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AgentProfileView {
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub client_type: Option<String>,
    pub client_key: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub system_prompt: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpsertAgentProfileRequest {
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub client_type: Option<String>,
    pub client_key: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileLaunchConfig {
    pub client_type: Option<String>,
    pub client_key: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub system_prompt: Option<String>,
}

/// Launch selection data for a Profile; the Profile system prompt is deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchableAgentProfile {
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub client_type: String,
    pub client_key: String,
    pub model: String,
    pub reasoning_effort: Option<String>,
}

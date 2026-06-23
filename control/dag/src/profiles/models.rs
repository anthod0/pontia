use super::*;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExecutionProfileView {
    pub profile_id: String,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub supported_client_types: Vec<String>,
    pub agent_kind: String,
    pub system_prompt_template: Option<String>,
    pub turn_prompt_template: Option<String>,
    pub default_session_role: Option<String>,
    pub default_session_description: Option<String>,
    pub handle_prefix: Option<String>,
    pub expected_output_schema: Option<String>,
    pub artifact_contract: Value,
    pub default_execution_policy: Value,
    pub default_review_policy: Value,
    pub metadata: Value,
    pub active: bool,
    pub archived_at: Option<String>,
    pub archived_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UpsertExecutionProfileRequest {
    pub profile_id: String,
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(default)]
    pub supported_client_types: Vec<String>,
    pub agent_kind: String,
    pub system_prompt_template: Option<String>,
    pub turn_prompt_template: Option<String>,
    pub default_session_role: Option<String>,
    pub default_session_description: Option<String>,
    pub handle_prefix: Option<String>,
    pub expected_output_schema: Option<String>,
    #[serde(default = "empty_object")]
    pub artifact_contract: Value,
    #[serde(default = "empty_object")]
    pub default_execution_policy: Value,
    #[serde(default = "empty_object")]
    pub default_review_policy: Value,
    #[serde(default = "empty_object")]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentProfileCommandOutcome {
    pub data: Value,
    pub duplicate: bool,
}

fn empty_object() -> Value {
    json!({})
}

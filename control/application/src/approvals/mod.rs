use serde::Deserialize;
use serde_json::Value;
use tokio::sync::oneshot;

mod commands;
mod coordinator;
mod observations;
mod registration;
mod validation;

pub use commands::ApprovalCommandService;
pub use coordinator::ApprovalCoordinator;
pub use observations::{ApprovalObservationService, ClaudeToolDecisionObservation};
pub use registration::ApprovalRegistrationService;

pub const MAX_PERMISSION_SUGGESTIONS: usize = 8;
pub const MAX_PERMISSION_RULES: usize = 16;
pub const MAX_PERMISSION_DIRECTORIES: usize = 16;
pub const MAX_APPROVAL_STRING_CHARS: usize = 512;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRegistrationRequest {
    pub session_id: String,
    pub prompt_id: Option<String>,
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Value,
    #[serde(default)]
    pub permission_suggestions: Vec<Value>,
    #[serde(default)]
    pub hook_input: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalWaitOutcome {
    ResolvedElsewhere,
    AcceptOnce,
    Reject,
    AlwaysAllow { permission_suggestion: Value },
}

impl ApprovalWaitOutcome {
    pub fn response_value(&self) -> Value {
        match self {
            Self::ResolvedElsewhere => Value::String("resolved_elsewhere".to_string()),
            Self::AcceptOnce => serde_json::json!({"decision": "accept_once"}),
            Self::Reject => serde_json::json!({"decision": "reject"}),
            Self::AlwaysAllow {
                permission_suggestion,
            } => serde_json::json!({
                "decision": "always_allow",
                "permission_suggestion": permission_suggestion,
            }),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub enum ApprovalDecisionRequest {
    AcceptOnce,
    Reject,
    AlwaysAllow { permission_suggestion: Value },
}

pub struct PendingApproval {
    pub request_event_id: String,
    pub session_id: String,
    pub turn_id: String,
    receiver: oneshot::Receiver<ApprovalWaitOutcome>,
}

impl PendingApproval {
    pub async fn wait(self) -> ApprovalWaitOutcome {
        self.receiver
            .await
            .unwrap_or(ApprovalWaitOutcome::ResolvedElsewhere)
    }
}

#[cfg(test)]
mod tests;

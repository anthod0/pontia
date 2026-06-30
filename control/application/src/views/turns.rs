use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnInputView {
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnOutputView {
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnView {
    pub turn_id: String,
    pub session_id: String,
    pub state: String,
    pub input: TurnInputView,
    pub output: TurnOutputView,
    pub failure: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub metadata: Value,
}

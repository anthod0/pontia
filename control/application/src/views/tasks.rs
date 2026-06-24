use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskView {
    pub task_id: String,
    pub state: String,
    pub input: String,
    pub workspace_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub routing_state: String,
    pub routing_reason: Option<String>,
    pub routing_confidence: Option<f64>,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskEventView {
    pub event_id: String,
    pub task_id: String,
    pub event_type: String,
    pub payload: Value,
    pub created_at: String,
}

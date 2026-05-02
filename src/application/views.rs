use super::*;

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCapabilities {
    pub accept_task: bool,
    pub report_turn_started: bool,
    pub report_turn_finished: bool,
    pub interrupt: bool,
    pub stream_output: bool,
    pub heartbeat: bool,
    pub artifact_sources: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionView {
    pub session_id: String,
    pub client_type: String,
    pub state: String,
    pub current_turn_id: Option<String>,
    pub workspace_id: Option<String>,
    pub workspace: Option<String>,
    pub capabilities: SessionCapabilities,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkspaceView {
    pub workspace_id: String,
    pub canonical_path: String,
    pub display_path: String,
    pub name: Option<String>,
    pub state: String,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

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
pub struct TurnInputView {
    pub summary: Option<String>,
    pub artifact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnOutputView {
    pub summary: Option<String>,
    pub artifact_ids: Vec<String>,
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

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventView {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub time: String,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventStreamItem {
    pub rowid: i64,
    pub event: EventView,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStreamScope<'a> {
    Session {
        session_id: &'a str,
    },
    Turn {
        session_id: &'a str,
        turn_id: &'a str,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArtifactView {
    pub artifact_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub size_bytes: Option<i64>,
    pub preview: Option<String>,
    pub created_at: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactContent {
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArtifactDiscoveryOutcome {
    pub artifacts: Vec<ArtifactView>,
}

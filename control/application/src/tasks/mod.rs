use super::*;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateDagTaskRequest {
    pub input: String,
    pub workspace: Option<String>,
    pub workspace_id: Option<String>,
    #[serde(default = "default_client_type")]
    pub client_type: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct HumanSignalRequest {
    pub kind: String,
    pub summary: String,
    pub detail: Option<String>,
    #[serde(default = "default_signal_severity")]
    pub severity: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTaskOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct TaskCommandService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
}

mod commands;
mod persistence;

impl TaskCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            graph: GraphRuntimeConfig::default(),
        }
    }

    pub fn with_runtime(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self { pool, graph }
    }
}

pub(super) fn is_terminal_task_state(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}

fn default_signal_severity() -> String {
    "medium".to_string()
}

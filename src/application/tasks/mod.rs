use super::*;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateTaskRequest {
    pub input: String,
    pub workspace: Option<String>,
    #[serde(default = "default_client_type")]
    pub client_type: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ConfirmTaskWorkspaceRequest {
    pub workspace: String,
    #[serde(default = "default_client_type")]
    pub client_type: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateDagTaskRequest {
    pub input: String,
    pub workspace: Option<String>,
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
    planner: PlannerRuntimeConfig,
    graph: GraphRuntimeConfig,
}

mod commands;
mod dispatch;
mod persistence;
mod planning;

impl TaskCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            planner: PlannerRuntimeConfig::default(),
            graph: GraphRuntimeConfig::default(),
        }
    }

    pub fn with_planner(pool: SqlitePool, planner: PlannerRuntimeConfig) -> Self {
        Self {
            pool,
            planner,
            graph: GraphRuntimeConfig::default(),
        }
    }

    pub fn with_runtime(
        pool: SqlitePool,
        planner: PlannerRuntimeConfig,
        graph: GraphRuntimeConfig,
    ) -> Self {
        Self {
            pool,
            planner,
            graph,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum DispatchRoutingUpdate {
    Matched,
    Confirmed,
}

pub(super) fn is_terminal_task_state(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}

fn default_signal_severity() -> String {
    "medium".to_string()
}

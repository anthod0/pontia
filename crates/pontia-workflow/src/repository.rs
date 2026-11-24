use pontia_agent_control::ControlQueryService;
use sqlx::SqlitePool;
mod projection;
mod terminal;

mod definition;
mod events;
mod lifecycle;
mod nodes;
mod queries;
mod recovery;
mod reports;
pub use recovery::WorkflowRecoveryCandidate;

#[derive(Debug, Clone)]
pub struct CreateWorkflowRecord {
    pub initiator_session_id: String,
    pub workflow_id: String,
    pub title: String,
    pub cwd: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct CreateWorkflowNodeRecord {
    pub node_id: String,
    pub workflow_id: String,
    pub parent_node_id: Option<String>,
    pub phase: String,
    pub title: String,
    pub instructions: String,
    pub inputs: String,
    pub output: String,
    pub execution_profile_id: Option<String>,
}

#[derive(Clone)]
pub struct SqliteWorkflowRepository {
    pool: SqlitePool,
    queries: ControlQueryService,
    recovery_client_types: Vec<String>,
}

impl SqliteWorkflowRepository {
    pub fn new(pool: SqlitePool, queries: ControlQueryService) -> Self {
        Self {
            pool,
            queries,
            recovery_client_types: Vec::new(),
        }
    }

    pub fn with_recovery_client_types(
        mut self,
        clients: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.recovery_client_types = clients.into_iter().map(Into::into).collect();
        self
    }
}

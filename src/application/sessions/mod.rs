use super::*;

mod commands;
mod dispatch;
mod persistence;
mod validation;

pub(crate) use validation::pilotfy_agent_kind;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateSessionRequest {
    #[serde(default = "default_client_type")]
    pub client_type: String,
    pub workspace: Option<String>,
    pub workspace_id: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub initial_task: Option<InitialTaskRequest>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct InitialTaskRequest {
    pub input: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateSessionOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct SessionCommandService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl SessionCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }
}

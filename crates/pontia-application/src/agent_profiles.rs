use sqlx::SqlitePool;

mod commands;
mod models;
mod queries;
mod rows;
mod validation;

pub use models::{AgentProfileCommandOutcome, ExecutionProfileView, UpsertExecutionProfileRequest};

#[derive(Clone)]
pub struct AgentProfileService {
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
}

impl AgentProfileService {
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            clients: Default::default(),
        }
    }
}

use super::*;

mod commands;
mod idempotency;
mod models;
mod queries;
mod rows;
mod validation;

pub use models::{AgentProfileCommandOutcome, ExecutionProfileView, UpsertExecutionProfileRequest};

#[derive(Clone)]
pub struct AgentProfileService {
    pool: SqlitePool,
}

impl AgentProfileService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

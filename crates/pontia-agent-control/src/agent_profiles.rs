use sqlx::SqlitePool;

mod commands;
mod launch;
mod models;
mod queries;
mod rows;
mod validation;

pub use models::{
    AgentProfileView, LaunchableAgentProfile, ProfileLaunchConfig, UpsertAgentProfileRequest,
};

#[derive(Clone)]
pub struct AgentProfileService {
    changes: crate::ControlChanges,
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
}

impl AgentProfileService {
    pub fn new(
        pool: SqlitePool,
        clients: crate::clients::ClientRegistry,
        changes: crate::ControlChanges,
    ) -> Self {
        Self {
            pool,
            clients,
            changes,
        }
    }
}

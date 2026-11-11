mod events;
mod git_status;
mod sessions;
pub use sessions::{SessionOverviewRequest, SessionOverviewView};
use sqlx::SqlitePool;
#[derive(Clone)]
pub struct ExternalQueryService {
    pool: SqlitePool,
    control: pontia_agent_control::ControlQueryService,
}
impl ExternalQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        let control = pontia_agent_control::ControlQueryService::new(pool.clone());
        Self { pool, control }
    }
    pub fn with_control_queries(
        mut self,
        control: pontia_agent_control::ControlQueryService,
    ) -> Self {
        self.control = control;
        self
    }
}

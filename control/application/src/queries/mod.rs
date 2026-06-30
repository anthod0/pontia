use super::*;

mod events;
mod git_status;
mod sessions;
mod tasks;
mod turns;
mod workspaces;

#[derive(Clone)]
pub struct ExternalQueryService {
    pool: SqlitePool,
}

impl ExternalQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn with_graph(pool: SqlitePool, _graph: pontia_config::GraphRuntimeConfig) -> Self {
        Self { pool }
    }
}

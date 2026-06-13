use super::*;

mod artifacts;
mod dag;
mod events;
mod git_status;
mod sessions;
mod tasks;
mod turns;
mod workspaces;

#[derive(Clone)]
pub struct ExternalQueryService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
}

impl ExternalQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_graph(pool, GraphRuntimeConfig::default())
    }

    pub fn with_graph(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self { graph, pool }
    }
}

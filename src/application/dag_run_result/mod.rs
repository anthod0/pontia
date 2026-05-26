use super::*;

mod aggregate;
mod payload;
mod runs;
mod signals;
mod submit;
mod types;

use payload::*;
use types::*;

#[derive(Clone)]
pub struct DagRunResultService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
}

impl DagRunResultService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_graph(pool, GraphRuntimeConfig::default())
    }

    pub fn with_graph(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self { pool, graph }
    }
}

use sqlx::SqlitePool;

use crate::error::Result;

use super::{GraphRuntimeConfig, TaskProvenance};

#[derive(Clone)]
pub struct GraphProjectionService {
    _pool: SqlitePool,
    _config: GraphRuntimeConfig,
}

impl GraphProjectionService {
    pub fn new(pool: SqlitePool, config: GraphRuntimeConfig) -> Self {
        Self {
            _pool: pool,
            _config: config,
        }
    }

    pub async fn project_task(&self, _task_id: &str) -> Result<()> {
        Ok(())
    }

    pub async fn task_provenance(&self, _task_id: &str) -> Result<TaskProvenance> {
        Ok(TaskProvenance {
            nodes: vec![],
            edges: vec![],
        })
    }
}

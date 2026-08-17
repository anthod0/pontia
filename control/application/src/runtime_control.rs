use pontia_runtime::GenericRuntimeManager;
use serde_json::Value;
use sqlx::SqlitePool;

mod commands;
mod persistence;

pub(crate) use persistence::runtime_binding_record;

#[derive(Debug, Clone, PartialEq)]
pub struct ControlCommandOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct RuntimeControlService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl RuntimeControlService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }
}

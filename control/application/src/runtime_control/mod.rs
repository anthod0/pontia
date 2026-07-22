use super::*;
mod commands;
mod persistence;

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

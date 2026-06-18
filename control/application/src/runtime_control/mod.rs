use super::{sessions::pontia_agent_kind, *};
use pontia_agent_clients::ReadinessMode;

mod commands;
mod idempotency;
mod persistence;
mod validation;

use validation::client_readiness_mode;

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

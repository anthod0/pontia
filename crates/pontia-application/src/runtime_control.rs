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
    event_ingest: crate::EventIngestService,
    runtime: GenericRuntimeManager,
}

impl RuntimeControlService {
    pub fn new(event_ingest: crate::EventIngestService) -> Self {
        Self {
            pool: event_ingest.db(),
            event_ingest,
            runtime: GenericRuntimeManager,
        }
    }
}

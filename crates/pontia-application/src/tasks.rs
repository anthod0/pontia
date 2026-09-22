use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTaskOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct TaskCommandService {
    pool: SqlitePool,
    event_ingest: crate::EventIngestService,
}

mod commands;
mod persistence;

impl TaskCommandService {
    pub fn new(event_ingest: crate::EventIngestService) -> Self {
        Self {
            pool: event_ingest.db(),
            event_ingest,
        }
    }
}

pub(super) fn is_terminal_task_state(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}

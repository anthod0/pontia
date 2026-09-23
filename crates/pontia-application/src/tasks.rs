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
    turns: crate::TurnCommandService,
}

mod commands;
mod persistence;

impl TaskCommandService {
    pub(crate) fn new(pool: SqlitePool, turns: crate::TurnCommandService) -> Self {
        Self { pool, turns }
    }
}

pub(super) fn is_terminal_task_state(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}

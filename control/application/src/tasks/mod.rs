use super::*;

#[derive(Debug, Clone, PartialEq)]
pub struct CreateTaskOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct TaskCommandService {
    pool: SqlitePool,
}

mod commands;
mod persistence;

impl TaskCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

pub(super) fn is_terminal_task_state(state: &str) -> bool {
    matches!(state, "completed" | "failed" | "cancelled")
}

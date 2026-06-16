use sqlx::SqlitePool;

use crate::{
    error::Result,
    storage::sqlite::models::tasks::{TaskEventRow, TaskRow},
};

#[derive(Debug, Clone)]
pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

impl SqliteTaskRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskRow>> {
        Ok(sqlx::query_as::<_, TaskRow>(
            r#"SELECT task_id, state, input, workspace_id, session_id, turn_id,
                      routing_state, routing_reason, routing_confidence, metadata,
                      created_at, updated_at
               FROM tasks ORDER BY created_at DESC, task_id"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskRow>> {
        Ok(sqlx::query_as::<_, TaskRow>(
            r#"SELECT task_id, state, input, workspace_id, session_id, turn_id,
                      routing_state, routing_reason, routing_confidence, metadata,
                      created_at, updated_at
               FROM tasks WHERE task_id = ?"#,
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_task_events(&self, task_id: &str) -> Result<Vec<TaskEventRow>> {
        Ok(sqlx::query_as::<_, TaskEventRow>(
            r#"SELECT event_id, task_id, event_type, payload, created_at
               FROM task_events WHERE task_id = ? ORDER BY created_at, event_id"#,
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?)
    }
}

use sqlx::SqlitePool;

use crate::{
    error::Result,
    storage::sqlite::models::tasks::{TaskEventRow, TaskRow},
};

#[derive(Debug, Clone)]
pub struct CreateTaskRecord {
    pub task_id: String,
    pub state: String,
    pub input: String,
    pub workspace_id: Option<String>,
    pub routing_state: String,
    pub routing_confidence: Option<f64>,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct SqliteTaskRepository {
    pool: SqlitePool,
}

impl SqliteTaskRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create_task(&self, task: CreateTaskRecord) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO tasks (task_id, state, input, workspace_id, routing_state, routing_confidence, metadata)
               VALUES (?, ?, ?, ?, ?, ?, ?)"#,
            task.task_id,
            task.state,
            task.input,
            task.workspace_id,
            task.routing_state,
            task.routing_confidence,
            task.metadata,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn update_task_state(&self, task_id: &str, state: &str) -> Result<()> {
        sqlx::query!(
            r#"UPDATE tasks
               SET state = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE task_id = ?"#,
            state,
            task_id,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_task_event(
        &self,
        event_id: &str,
        task_id: &str,
        event_type: &str,
        payload: &str,
    ) -> Result<()> {
        sqlx::query!(
            r#"INSERT INTO task_events (event_id, task_id, event_type, payload)
               VALUES (?, ?, ?, ?)"#,
            event_id,
            task_id,
            event_type,
            payload,
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_tasks(&self) -> Result<Vec<TaskRow>> {
        Ok(sqlx::query_as!(
            TaskRow,
            r#"SELECT task_id, state, input, workspace_id, session_id, turn_id,
                      routing_state, routing_reason, routing_confidence, metadata,
                      created_at, updated_at
               FROM tasks ORDER BY created_at DESC, task_id"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskRow>> {
        Ok(sqlx::query_as!(
            TaskRow,
            r#"SELECT task_id, state, input, workspace_id, session_id, turn_id,
                      routing_state, routing_reason, routing_confidence, metadata,
                      created_at, updated_at
               FROM tasks WHERE task_id = ?"#,
            task_id,
        )
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn list_task_events(&self, task_id: &str) -> Result<Vec<TaskEventRow>> {
        Ok(sqlx::query_as!(
            TaskEventRow,
            r#"SELECT event_id, task_id, event_type, payload, created_at
               FROM task_events WHERE task_id = ? ORDER BY created_at, event_id"#,
            task_id,
        )
        .fetch_all(&self.pool)
        .await?)
    }
}

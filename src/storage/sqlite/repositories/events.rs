use sqlx::SqlitePool;

use pontia_storage_sqlite::models::events::{EventRow, EventStreamRow, TaskEventStreamRow};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct SqliteEventRepository {
    pool: SqlitePool,
}

impl SqliteEventRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_session_events(&self, session_id: &str) -> Result<Vec<EventRow>> {
        Ok(sqlx::query_as::<_, EventRow>(
            r#"SELECT event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_turn_events(&self, session_id: &str, turn_id: &str) -> Result<Vec<EventRow>> {
        Ok(sqlx::query_as::<_, EventRow>(
            r#"SELECT event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND turn_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn resolve_session_event_cursor(
        &self,
        session_id: &str,
        event_id: &str,
    ) -> Result<Option<i64>> {
        Ok(
            sqlx::query_scalar("SELECT rowid FROM events WHERE session_id = ? AND event_id = ?")
                .bind(session_id)
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn resolve_turn_event_cursor(
        &self,
        session_id: &str,
        turn_id: &str,
        event_id: &str,
    ) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT rowid FROM events WHERE session_id = ? AND turn_id = ? AND event_id = ?",
        )
        .bind(session_id)
        .bind(turn_id)
        .bind(event_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn current_session_stream_rowid(&self) -> Result<i64> {
        Ok(
            sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(rowid) FROM events")
                .fetch_one(&self.pool)
                .await?
                .unwrap_or(0),
        )
    }

    pub async fn current_task_stream_rowid(&self) -> Result<i64> {
        Ok(
            sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(rowid) FROM task_events")
                .fetch_one(&self.pool)
                .await?
                .unwrap_or(0),
        )
    }

    pub async fn list_session_stream_rows_after(
        &self,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<EventStreamRow>> {
        Ok(sqlx::query_as::<_, EventStreamRow>(
            r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE rowid > ? ORDER BY rowid LIMIT ?"#,
        )
        .bind(after_rowid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_task_stream_rows_after(
        &self,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<TaskEventStreamRow>> {
        Ok(sqlx::query_as::<_, TaskEventStreamRow>(
            r#"SELECT rowid, event_id, task_id, event_type, payload, created_at
               FROM task_events WHERE rowid > ? ORDER BY rowid LIMIT ?"#,
        )
        .bind(after_rowid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_session_event_stream_rows_after(
        &self,
        session_id: &str,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<EventStreamRow>> {
        Ok(sqlx::query_as::<_, EventStreamRow>(
            r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND rowid > ? ORDER BY rowid LIMIT ?"#,
        )
        .bind(session_id)
        .bind(after_rowid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn list_turn_event_stream_rows_after(
        &self,
        session_id: &str,
        turn_id: &str,
        after_rowid: i64,
        limit: i64,
    ) -> Result<Vec<EventStreamRow>> {
        Ok(sqlx::query_as::<_, EventStreamRow>(
            r#"SELECT rowid, event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND turn_id = ? AND rowid > ? ORDER BY rowid LIMIT ?"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .bind(after_rowid)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }
}

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::models::events::{DomainEventRow, EventRow, EventStreamRow, TaskEventStreamRow};

use pontia_core::Result;

#[derive(Debug, Clone)]
pub struct EventInsertRecord {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    pub client_type: String,
    pub event_type: String,
    pub occurred_at: String,
    pub seq: Option<i64>,
    pub payload: String,
    pub timeline_boundary: Option<String>,
    pub turn_topology: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SqliteEventRepository {
    pool: SqlitePool,
}

impl SqliteEventRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn insert_event_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        event: EventInsertRecord,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO events
               (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, seq, payload, timeline_boundary, turn_topology)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(event.event_id)
        .bind(event.session_id)
        .bind(event.turn_id)
        .bind(event.source)
        .bind(event.client_type)
        .bind(event.event_type)
        .bind(event.occurred_at)
        .bind(event.seq)
        .bind(event.payload)
        .bind(event.timeline_boundary)
        .bind(event.turn_topology)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    pub async fn existing_event_state_version(
        &self,
        event_id: &str,
        session_id: &str,
    ) -> Result<Option<i64>> {
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM events WHERE event_id = ?")
            .bind(event_id)
            .fetch_optional(&self.pool)
            .await?;

        if exists.is_none() {
            return Ok(None);
        }

        Ok(Some(
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?,
        ))
    }

    pub async fn session_event_count_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        session_id: &str,
    ) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
                .bind(session_id)
                .fetch_one(&mut **tx)
                .await?,
        )
    }

    pub async fn session_event_count(&self, session_id: &str) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn list_domain_event_rows(&self, session_id: &str) -> Result<Vec<DomainEventRow>> {
        Ok(sqlx::query_as::<_, DomainEventRow>(
            r#"SELECT event_id, session_id, turn_id, source, client_type, event_type, occurred_at, seq, payload, timeline_boundary, turn_topology
               FROM events WHERE session_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn max_seq(&self, session_id: &str) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT MAX(seq) FROM events WHERE session_id = ? AND seq IS NOT NULL",
        )
        .bind(session_id)
        .fetch_one(&self.pool)
        .await?)
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

    pub async fn ready_payloads(&self, session_id: &str, client_type: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            r#"SELECT payload FROM events
               WHERE session_id = ?
                 AND event_type = 'session.ready'
                 AND source = 'agent_client'
                 AND client_type = ?"#,
        )
        .bind(session_id)
        .bind(client_type)
        .fetch_all(&self.pool)
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect_sqlite, run_migrations};

    async fn pool() -> sqlx::SqlitePool {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("events.db");
        let _kept_dir = dir.keep();
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        db
    }

    #[tokio::test]
    async fn counts_persisted_events_for_session_without_transaction() {
        let pool = pool().await;
        sqlx::query(
            r#"INSERT INTO events
               (event_id, session_id, source, client_type, event_type, occurred_at, payload)
               VALUES
               ('evt_1', 'sess_1', 'external_api', 'pi', 'session.created', '2026-01-01T00:00:00Z', '{}'),
               ('evt_2', 'sess_1', 'external_api', 'pi', 'session.started', '2026-01-01T00:00:01Z', '{}'),
               ('evt_3', 'sess_2', 'external_api', 'pi', 'session.created', '2026-01-01T00:00:02Z', '{}')"#,
        )
        .execute(&pool)
        .await
        .unwrap();

        let count = SqliteEventRepository::new(pool)
            .session_event_count("sess_1")
            .await
            .unwrap();

        assert_eq!(count, 2);
    }
}

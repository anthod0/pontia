use pontia_core::Result;
use sqlx::SqlitePool;

use crate::models::inbox::{InboxMessageRow, PendingInboxMessageRow};

#[derive(Debug, Clone)]
pub struct SqliteInboxRepository {
    pool: SqlitePool,
}

impl SqliteInboxRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn supersede_pending_interrupts(
        &self,
        session_id: &str,
        superseded_by_message_id: &str,
    ) -> Result<u64> {
        Ok(sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'superseded', superseded_by_message_id = ?,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE session_id = ? AND delivery_policy = 'interrupt_now' AND state = 'pending'"#,
        )
        .bind(superseded_by_message_id)
        .bind(session_id)
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn insert_message(
        &self,
        message_id: &str,
        session_id: &str,
        delivery_policy: &str,
        input_summary: &str,
        metadata: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO inbox_messages
               (message_id, session_id, state, delivery_policy, input_summary, metadata)
               VALUES (?, ?, 'pending', ?, ?, ?)"#,
        )
        .bind(message_id)
        .bind(session_id)
        .bind(delivery_policy)
        .bind(input_summary)
        .bind(metadata)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<InboxMessageRow>> {
        Ok(
            sqlx::query_as::<_, InboxMessageRow>(SELECT_INBOX_MESSAGE_SQL_WITH_SESSION)
                .bind(session_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn get_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<Option<InboxMessageRow>> {
        Ok(
            sqlx::query_as::<_, InboxMessageRow>(SELECT_INBOX_MESSAGE_SQL_WITH_SESSION_AND_MESSAGE)
                .bind(session_id)
                .bind(message_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn cancel_pending_message(&self, session_id: &str, message_id: &str) -> Result<u64> {
        Ok(sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'cancelled', cancelled_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE session_id = ? AND message_id = ? AND state = 'pending'"#,
        )
        .bind(session_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn dismiss_failed_message(&self, session_id: &str, message_id: &str) -> Result<u64> {
        Ok(sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'dismissed', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE session_id = ? AND message_id = ? AND state = 'failed'"#,
        )
        .bind(session_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn next_pending_message(
        &self,
        session_id: &str,
    ) -> Result<Option<PendingInboxMessageRow>> {
        Ok(sqlx::query_as::<_, PendingInboxMessageRow>(
            r#"SELECT message_id, input_summary, metadata
               FROM inbox_messages
               WHERE session_id = ? AND state = 'pending'
               ORDER BY CASE WHEN delivery_policy = 'interrupt_now' THEN 0 ELSE 1 END,
                        CASE WHEN delivery_policy = 'interrupt_now' THEN created_at END DESC,
                        CASE WHEN delivery_policy = 'interrupt_now' THEN message_id END DESC,
                        created_at ASC,
                        message_id ASC
               LIMIT 1"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn mark_dispatching(&self, message_id: &str) -> Result<u64> {
        Ok(sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'dispatching', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE message_id = ? AND state = 'pending'"#,
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn mark_dispatched(&self, message_id: &str, turn_id: Option<&str>) -> Result<()> {
        sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'dispatched', turn_id = ?, dispatched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE message_id = ?"#,
        )
        .bind(turn_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_failed(&self, message_id: &str, failure_message: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'failed', failure_message = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE message_id = ? AND state IN ('pending', 'dispatching')"#,
        )
        .bind(failure_message)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn link_started_turn(
        &self,
        session_id: &str,
        message_id: &str,
        turn_id: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE inbox_messages
               SET turn_id = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE session_id = ? AND message_id = ? AND turn_id IS NULL"#,
        )
        .bind(turn_id)
        .bind(session_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

const SELECT_INBOX_MESSAGE_SQL_WITH_SESSION: &str = r#"SELECT message_id, session_id, state, delivery_policy, input_summary, metadata,
          turn_id, superseded_by_message_id, failure_message, created_at, updated_at,
          dispatched_at, cancelled_at
   FROM inbox_messages WHERE session_id = ? ORDER BY created_at, message_id"#;

const SELECT_INBOX_MESSAGE_SQL_WITH_SESSION_AND_MESSAGE: &str = r#"SELECT message_id, session_id, state, delivery_policy, input_summary, metadata,
          turn_id, superseded_by_message_id, failure_message, created_at, updated_at,
          dispatched_at, cancelled_at
   FROM inbox_messages WHERE session_id = ? AND message_id = ?"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect_sqlite, run_migrations};

    async fn pool() -> sqlx::SqlitePool {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("inbox.db");
        let _kept_dir = dir.keep();
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        db
    }

    #[tokio::test]
    async fn queues_interrupt_message_and_supersedes_previous_pending_interrupts() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_1', 'pi', 'idle')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let repo = SqliteInboxRepository::new(pool);

        repo.insert_message("msg_1", "sess_1", "interrupt_now", "one", "{}")
            .await
            .unwrap();
        repo.supersede_pending_interrupts("sess_1", "msg_2")
            .await
            .unwrap();
        repo.insert_message("msg_2", "sess_1", "interrupt_now", "two", "{}")
            .await
            .unwrap();

        let first = repo.get_message("sess_1", "msg_1").await.unwrap().unwrap();
        let second = repo.get_message("sess_1", "msg_2").await.unwrap().unwrap();
        assert_eq!(first.state, "superseded");
        assert_eq!(first.superseded_by_message_id.as_deref(), Some("msg_2"));
        assert_eq!(second.state, "pending");
    }
}

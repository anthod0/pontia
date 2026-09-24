use pontia_core::Result;
use sqlx::SqlitePool;

use crate::models::inbox::{InboxMessageRow, PendingInboxMessageRow};

pub struct NewInboxMessage<'a> {
    pub message_id: &'a str,
    pub session_id: &'a str,
    pub delivery_policy: &'a str,
    pub input: &'a str,
    pub metadata: &'a str,
    pub branch_target: Option<&'a str>,
    pub steer_target: Option<&'a str>,
    pub submission_payload: &'a str,
    pub retry_of: Option<&'a str>,
    pub resuming: bool,
}

#[derive(Debug, Clone)]
pub struct SqliteInboxRepository {
    pool: SqlitePool,
}

impl SqliteInboxRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn enqueue(&self, message: NewInboxMessage<'_>) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO inbox_messages (message_id,session_id,state,delivery_policy,input_summary,metadata,branch_target_turn_id,steer_target_turn_id,submission_payload,retry_of_message_id) VALUES (?,?,?,?,?,?,?,?,?,?) ON CONFLICT(message_id) DO NOTHING"
        )
        .bind(message.message_id).bind(message.session_id)
        .bind(if message.resuming { "resuming" } else { "pending" })
        .bind(message.delivery_policy).bind(message.input).bind(message.metadata)
        .bind(message.branch_target).bind(message.steer_target)
        .bind(message.submission_payload).bind(message.retry_of)
        .execute(&mut *tx).await?.rows_affected() == 1;
        if inserted && message.delivery_policy == "interrupt_now" {
            sqlx::query("UPDATE inbox_messages SET state='superseded',superseded_by_message_id=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE session_id=? AND delivery_policy='interrupt_now' AND state='pending' AND message_id<>?")
                .bind(message.message_id).bind(message.session_id).bind(message.message_id)
                .execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(inserted)
    }

    pub async fn return_pending(&self, message_id: &str) -> Result<()> {
        sqlx::query("UPDATE inbox_messages SET state='pending',failure_message=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE message_id=? AND state IN ('dispatching','resuming')")
            .bind(message_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn mark_unknown(&self, message_id: &str, reason: &str) -> Result<u64> {
        Ok(sqlx::query("UPDATE inbox_messages SET state=CASE WHEN turn_id IS NULL THEN 'unknown' ELSE 'dispatched' END,failure_message=CASE WHEN turn_id IS NULL THEN ? ELSE NULL END,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE message_id=? AND state IN ('pending','dispatching')")
            .bind(reason).bind(message_id).execute(&self.pool).await?.rows_affected())
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
            r#"SELECT message_id, input_summary, metadata, branch_target_turn_id, delivery_policy, steer_target_turn_id
               FROM inbox_messages
               WHERE session_id = ? AND state = 'pending'
               ORDER BY CASE delivery_policy WHEN 'interrupt_now' THEN 0 WHEN 'steer' THEN 1 ELSE 2 END,
                        CASE WHEN delivery_policy = 'interrupt_now' THEN created_at END DESC,
                        CASE WHEN delivery_policy = 'interrupt_now' THEN message_id END DESC,
                        rowid ASC
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
               WHERE message_id = ? AND state = 'pending'
                 AND NOT EXISTS (SELECT 1 FROM inbox_messages AS in_flight
                     WHERE in_flight.session_id = inbox_messages.session_id
                     AND (in_flight.state IN ('dispatching','resuming') OR (in_flight.state IN ('dispatched','unknown') AND in_flight.turn_id IS NULL AND NOT EXISTS (SELECT 1 FROM inbox_messages retry WHERE retry.retry_of_message_id=in_flight.message_id))))"#,
        )
        .bind(message_id)
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn mark_dispatched(&self, message_id: &str, turn_id: Option<&str>) -> Result<()> {
        sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'dispatched', turn_id = COALESCE(?, turn_id), dispatched_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE message_id = ? AND state = 'dispatching'"#,
        )
        .bind(turn_id)
        .bind(message_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn mark_failed(&self, message_id: &str, failure_message: &str) -> Result<u64> {
        Ok(sqlx::query(
            r#"UPDATE inbox_messages
               SET state = 'failed', failure_message = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE message_id = ? AND state IN ('pending', 'dispatching', 'resuming')"#,
        )
        .bind(failure_message)
        .bind(message_id)
        .execute(&self.pool)
        .await?.rows_affected())
    }

    pub async fn link_started_turn(
        &self,
        session_id: &str,
        message_id: &str,
        turn_id: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE inbox_messages
               SET turn_id = ?, state=CASE WHEN state='unknown' THEN 'dispatched' ELSE state END, failure_message=CASE WHEN state='unknown' THEN NULL ELSE failure_message END, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
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

const SELECT_INBOX_MESSAGE_SQL_WITH_SESSION: &str = r#"SELECT message_id, session_id, state, delivery_policy, input_summary, metadata, branch_target_turn_id,
          submission_payload, steer_target_turn_id, retry_of_message_id,
          (SELECT retry.message_id FROM inbox_messages retry WHERE retry.retry_of_message_id=inbox_messages.message_id) AS retried_by_message_id,
          turn_id, superseded_by_message_id, failure_message, created_at, updated_at,
          dispatched_at, cancelled_at
   FROM inbox_messages WHERE session_id = ? ORDER BY rowid"#;

const SELECT_INBOX_MESSAGE_SQL_WITH_SESSION_AND_MESSAGE: &str = r#"SELECT message_id, session_id, state, delivery_policy, input_summary, metadata, branch_target_turn_id,
          submission_payload, steer_target_turn_id, retry_of_message_id,
          (SELECT retry.message_id FROM inbox_messages retry WHERE retry.retry_of_message_id=inbox_messages.message_id) AS retried_by_message_id,
          turn_id, superseded_by_message_id, failure_message, created_at, updated_at,
          dispatched_at, cancelled_at
   FROM inbox_messages WHERE session_id = ? AND message_id = ?"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect_sqlite, run_migrations};

    fn message<'a>(
        id: &'a str,
        policy: &'a str,
        input: &'a str,
        target: Option<&'a str>,
    ) -> NewInboxMessage<'a> {
        NewInboxMessage {
            message_id: id,
            session_id: "sess_1",
            delivery_policy: policy,
            input,
            metadata: "{}",
            branch_target: target,
            steer_target: None,
            submission_payload: "{}",
            retry_of: None,
            resuming: false,
        }
    }

    async fn pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("inbox.db");
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        (db, dir)
    }

    #[tokio::test]
    async fn queues_interrupt_message_and_supersedes_previous_pending_interrupts() {
        let (pool, _pontia_home) = pool().await;
        sqlx::query(
            "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_1', 'pi', 'idle')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let repo = SqliteInboxRepository::new(pool);

        repo.enqueue(message("msg_1", "interrupt_now", "one", None))
            .await
            .unwrap();
        repo.enqueue(message("msg_2", "interrupt_now", "two", None))
            .await
            .unwrap();

        let first = repo.get_message("sess_1", "msg_1").await.unwrap().unwrap();
        let second = repo.get_message("sess_1", "msg_2").await.unwrap().unwrap();
        assert_eq!(first.state, "superseded");
        assert_eq!(first.superseded_by_message_id.as_deref(), Some("msg_2"));
        assert_eq!(second.state, "pending");
    }

    #[tokio::test]
    async fn round_trips_a_nullable_branch_target_turn() {
        let (pool, _pontia_home) = pool().await;
        sqlx::query(
            "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_1', 'pi', 'idle')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO turns (turn_id, session_id, state) VALUES ('turn_target', 'sess_1', 'completed')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let repo = SqliteInboxRepository::new(pool);

        repo.enqueue(message(
            "msg_branch",
            "after_idle",
            "replacement",
            Some("turn_target"),
        ))
        .await
        .unwrap();
        repo.enqueue(message("msg_plain", "after_idle", "ordinary", None))
            .await
            .unwrap();

        let branch = repo
            .get_message("sess_1", "msg_branch")
            .await
            .unwrap()
            .unwrap();
        let plain = repo
            .get_message("sess_1", "msg_plain")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(branch.branch_target_turn_id.as_deref(), Some("turn_target"));
        assert_eq!(plain.branch_target_turn_id, None);
    }
}

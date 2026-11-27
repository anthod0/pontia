use pontia_core::Result;
use sqlx::SqlitePool;

use crate::models::inbox::InboxMessageRow;

pub struct NewInboxMessage<'a> {
    pub message_id: &'a str,
    pub session_id: &'a str,
    pub input: &'a str,
    pub metadata: &'a str,
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
        Ok(sqlx::query(
            "INSERT INTO inbox_messages (message_id,session_id,input_summary,metadata) VALUES (?,?,?,?) ON CONFLICT(message_id) DO NOTHING",
        )
        .bind(message.message_id)
        .bind(message.session_id)
        .bind(message.input)
        .bind(message.metadata)
        .execute(&self.pool)
        .await?
        .rows_affected()
            == 1)
    }

    pub async fn list_messages(&self, session_id: &str) -> Result<Vec<InboxMessageRow>> {
        Ok(sqlx::query_as::<_, InboxMessageRow>(
            "SELECT message_id,session_id,input_summary,metadata,created_at FROM inbox_messages WHERE session_id=? ORDER BY created_at,rowid",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_message(
        &self,
        session_id: &str,
        message_id: &str,
    ) -> Result<Option<InboxMessageRow>> {
        Ok(sqlx::query_as::<_, InboxMessageRow>(
            "SELECT message_id,session_id,input_summary,metadata,created_at FROM inbox_messages WHERE session_id=? AND message_id=?",
        )
        .bind(session_id)
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn next_message(&self, session_id: &str) -> Result<Option<InboxMessageRow>> {
        Ok(sqlx::query_as::<_, InboxMessageRow>(
            "SELECT message_id,session_id,input_summary,metadata,created_at FROM inbox_messages WHERE session_id=? ORDER BY created_at,rowid LIMIT 1",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn has_pending_input(&self, session_id: &str) -> Result<bool> {
        Ok(
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM inbox_messages WHERE session_id=?)")
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn remove(&self, session_id: &str, message_id: &str) -> Result<u64> {
        Ok(
            sqlx::query("DELETE FROM inbox_messages WHERE session_id=? AND message_id=?")
                .bind(session_id)
                .bind(message_id)
                .execute(&self.pool)
                .await?
                .rows_affected(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{connect_sqlite, run_migrations};

    async fn pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("inbox.db");
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        (db, dir)
    }

    #[tokio::test]
    async fn removes_sent_messages_from_the_queue() {
        let (pool, _test_root) = pool().await;
        sqlx::query(
            "INSERT INTO sessions (session_id,client_type,state) VALUES ('session','pi','idle')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let repository = SqliteInboxRepository::new(pool);
        repository
            .enqueue(NewInboxMessage {
                message_id: "message",
                session_id: "session",
                input: "input",
                metadata: "{}",
            })
            .await
            .unwrap();

        assert_eq!(repository.remove("session", "message").await.unwrap(), 1);
        assert!(!repository.has_pending_input("session").await.unwrap());
    }
}

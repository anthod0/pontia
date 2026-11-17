use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum MessageKind {
    UserInput,
    BotReply,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::FromRow, PartialEq, Eq)]
pub struct BotMessage {
    pub id: String,
    pub bot_id: String,
    pub kind: MessageKind,
    pub content: String,
    pub created_at: String,
}

/// Visible chat storage. The conversation service owns input delivery and failures.
#[derive(Clone)]
pub struct MessageService {
    pool: SqlitePool,
}

impl MessageService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn append(
        &self,
        bot_id: &str,
        kind: MessageKind,
        content: &str,
    ) -> Result<BotMessage> {
        if content.trim().is_empty() {
            return Err(Error::Domain("message content cannot be empty".into()));
        }
        sqlx::query_as(
            "INSERT INTO bot_messages (id, bot_id, kind, content)
             SELECT ?, bot_id, ?, ? FROM bots WHERE bot_id = ? RETURNING *",
        )
        .bind(format!("bmsg_{}", uuid::Uuid::now_v7()))
        .bind(kind)
        .bind(content)
        .bind(bot_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Bot {bot_id} not found")))
    }

    pub async fn list(&self, bot_id: &str) -> Result<Vec<BotMessage>> {
        Ok(
            sqlx::query_as("SELECT * FROM bot_messages WHERE bot_id = ? ORDER BY created_at, id")
                .bind(bot_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }
}

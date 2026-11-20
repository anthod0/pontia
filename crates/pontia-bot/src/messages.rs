use pontia_agent_control::queries::TimePage;
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum MessageStatus {
    Normal,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::FromRow, PartialEq, Eq)]
pub struct BotMessage {
    pub id: String,
    pub bot_id: String,
    pub kind: MessageKind,
    pub content: String,
    pub created_at: String,
    pub status: MessageStatus,
}

#[derive(sqlx::FromRow)]
pub(crate) struct TimedMessage {
    pub timeline_ms: i64,
    #[sqlx(flatten)]
    pub message: BotMessage,
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
        append_message(&self.pool, bot_id, kind, content).await
    }

    pub(crate) async fn reply(
        &self,
        bot_id: &str,
        content: &str,
        working_on: Option<&str>,
    ) -> Result<BotMessage> {
        let mut tx = self.pool.begin().await?;
        let message = append_message(&mut *tx, bot_id, MessageKind::BotReply, content).await?;
        sqlx::query("UPDATE bots SET working_on = ? WHERE bot_id = ?")
            .bind(working_on)
            .bind(bot_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(message)
    }

    pub async fn list(&self, bot_id: &str) -> Result<Vec<BotMessage>> {
        Ok(
            sqlx::query_as("SELECT * FROM bot_messages WHERE bot_id = ? ORDER BY created_at, id")
                .bind(bot_id)
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub(crate) async fn page(
        &self,
        bot_id: &str,
        page: TimePage<'_>,
        limit: u32,
    ) -> Result<Vec<TimedMessage>> {
        let mut query = sqlx::QueryBuilder::new("SELECT * FROM bot_messages WHERE bot_id = ");
        query.push_bind(bot_id);
        if let TimePage::Before(ms, id) | TimePage::After(ms, id) = page {
            query.push(if matches!(page, TimePage::After(..)) {
                " AND (timeline_ms, id) > ("
            } else {
                " AND (timeline_ms, id) < ("
            });
            query.push_bind(ms).push(", ").push_bind(id).push(")");
        }
        query
            .push(if matches!(page, TimePage::After(..)) {
                " ORDER BY timeline_ms, id LIMIT "
            } else {
                " ORDER BY timeline_ms DESC, id DESC LIMIT "
            })
            .push_bind(limit);
        Ok(query.build_query_as().fetch_all(&self.pool).await?)
    }

    pub(crate) async fn find(&self, bot_id: &str, id: &str) -> Result<Option<TimedMessage>> {
        Ok(
            sqlx::query_as("SELECT * FROM bot_messages WHERE bot_id = ? AND id = ?")
                .bind(bot_id)
                .bind(id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub(crate) async fn unanswered(&self, bot_id: &str) -> Result<bool> {
        let latest: Option<MessageKind> = sqlx::query_scalar(
            "SELECT kind FROM bot_messages WHERE bot_id = ? AND status = 'normal'
             ORDER BY timeline_ms DESC, id DESC LIMIT 1",
        )
        .bind(bot_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(latest == Some(MessageKind::UserInput))
    }

    pub(crate) async fn retry(&self, bot_id: &str, id: &str) -> Result<BotMessage> {
        sqlx::query_as(
            "UPDATE bot_messages SET status = 'normal'
             WHERE bot_id = ? AND id = ? AND status = 'failed' RETURNING *",
        )
        .bind(bot_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            Error::StateConflict("Only failed messages in this Bot chat can be retried".into())
        })
    }

    pub(crate) async fn fail(&self, message: &BotMessage) -> Result<BotMessage> {
        Ok(sqlx::query_as(
            "UPDATE bot_messages SET status = 'failed'
             WHERE bot_id = ? AND id = ? RETURNING *",
        )
        .bind(&message.bot_id)
        .bind(&message.id)
        .fetch_one(&self.pool)
        .await?)
    }
}

async fn append_message<'a>(
    executor: impl sqlx::SqliteExecutor<'a>,
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
    .fetch_optional(executor)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Bot {bot_id} not found")))
}

mod links;
pub use links::MessageLinks;
mod search;
pub use search::SearchPage;
mod todos;
pub use todos::{TodoAction, TodoList};

use pontia_agent_control::queries::TimePage;
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::collections::BTreeSet;

use crate::conversation::{Reply, SentReply};

const MAX_RELATED: usize = 5;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Todo {
    Open {
        by: String,
        at: String,
    },
    Done {
        by: String,
        at: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message_id: Option<String>,
    },
}

pub(crate) const MESSAGE_COLUMNS: &str = "m.*, (
    SELECT json_object('state', state, 'by', coalesce(resolved_by, opened_by), 'at', coalesce(resolved_at, opened_at),
                       'message_id', resolved_message_id)
    FROM bot_todos WHERE message_id = m.id) AS todo";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, sqlx::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum MessageKind {
    UserInput,
    BotReply,
}

impl MessageKind {
    pub(crate) fn speaker(self) -> &'static str {
        match self {
            Self::UserInput => "User",
            Self::BotReply => "Bot",
        }
    }
}

pub(crate) fn summary(kind: MessageKind, mut content: &str, limit: usize) -> String {
    let mut attachments = Vec::new();
    if kind == MessageKind::UserInput {
        if content.starts_with("<user_quote ")
            && let Some((_, body)) = content.split_once("</user_quote>\n")
        {
            content = body;
        }
        for (open, close, child_close) in [
            ("<user_uploads ", "</user_uploads>", "</user_upload>"),
            ("<user_secrets ", "</user_secrets>", "</user_secret>"),
        ] {
            if content.starts_with(open)
                && let Some((block, body)) = content.split_once(close)
            {
                content = body.trim_start();
                for line in block.lines().skip(1) {
                    let Some((tag, text)) = line
                        .strip_suffix(child_close)
                        .and_then(|s| s.split_once('>'))
                    else {
                        continue;
                    };
                    if child_close == "</user_upload>" {
                        attachments.push(text.to_owned());
                        continue;
                    }
                    let visible = tag
                        .split_once(" label=\"")
                        .and_then(|(_, s)| s.strip_suffix('"'))
                        .unwrap_or(text);
                    attachments.push(
                        visible
                            .replace("&lt;", "<")
                            .replace("&gt;", ">")
                            .replace("&quot;", "\"")
                            .replace("&apos;", "'")
                            .replace("&#9;", " ")
                            .replace("&#10;", " ")
                            .replace("&#13;", " ")
                            .replace("&amp;", "&"),
                    );
                }
            }
        }
    }
    let fallback = attachments.join(", ");
    let body = if content.is_empty() {
        &fallback
    } else {
        content
    };
    body.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
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
    #[sqlx(default, json(nullable))]
    pub todo: Option<Todo>,
}

#[derive(Serialize)]
pub struct MessageContext {
    pub messages: Vec<BotMessage>,
    pub truncated: bool,
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
        let (content, quoted) = match kind {
            MessageKind::UserInput => crate::quote::user_input(content),
            MessageKind::BotReply => (content.into(), None),
        };
        let mut tx = self.pool.begin().await?;
        let message = append_message(
            &mut *tx,
            bot_id,
            kind,
            &content,
            quoted.as_ref().map(|_| "quote"),
        )
        .await?;
        if let Some(crate::quote::Target(target)) = quoted {
            link_message(&mut tx, &message, &target).await?;
        }
        tx.commit().await?;
        Ok(message)
    }

    pub(crate) async fn reply(&self, bot_id: &str, reply: &Reply) -> Result<SentReply> {
        if reply.related_to.len() > MAX_RELATED {
            return Err(Error::Domain(
                "at most five related messages are allowed".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let mut message = append_message(
            &mut *tx,
            bot_id,
            MessageKind::BotReply,
            &reply.message,
            None,
        )
        .await?;
        for target in reply
            .related_to
            .iter()
            .chain(&reply.resolve_todos)
            .collect::<BTreeSet<_>>()
        {
            if !link_message(&mut tx, &message, target).await? {
                return Err(Error::Domain(format!(
                    "related message {target} does not exist in this Bot chat"
                )));
            }
        }
        let mut resolved = Vec::new();
        let mut already_resolved = Vec::new();
        for target in reply.resolve_todos.iter().collect::<BTreeSet<_>>() {
            let updated = sqlx::query(
                "UPDATE bot_todos SET state = 'done', resolved_by = 'bot',
                    resolved_message_id = ?, resolved_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
                WHERE message_id = ? AND state = 'open'",
            )
            .bind(&message.id)
            .bind(target)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() == 1 {
                resolved.push(target.clone());
            } else if sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM bot_todos WHERE message_id = ? AND resolved_by = 'user')"
            ).bind(target).fetch_one(&mut *tx).await? {
                already_resolved.push(target.clone());
            } else {
                return Err(Error::Domain(format!("message {target} is not an open todo")));
            }
        }
        if reply.todo {
            let at = sqlx::query_scalar("INSERT INTO bot_todos(message_id, state, opened_by) VALUES (?, 'open', 'bot') RETURNING opened_at")
                .bind(&message.id)
                .fetch_one(&mut *tx)
                .await?;
            message.todo = Some(Todo::Open {
                by: "bot".into(),
                at,
            });
        }
        sqlx::query("UPDATE bots SET awaiting_reply = 0 WHERE bot_id = ?")
            .bind(bot_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(SentReply {
            message,
            resolved,
            already_resolved,
        })
    }

    pub(crate) async fn open_todo_count(&self, bot_id: &str) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT count(*) FROM bot_todos t JOIN bot_messages m ON m.id = t.message_id
            WHERE m.bot_id = ? AND t.state = 'open'",
        )
        .bind(bot_id)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn context(&self, bot_id: &str, id: &str) -> Result<MessageContext> {
        let mut rows: Vec<TimedMessage> = sqlx::query_as(
            &format!("WITH RECURSIVE
                selected AS (SELECT id, timeline_ms FROM bot_messages WHERE bot_id = ? AND id = ?),
                ancestors(id) AS (
                    SELECT id FROM selected UNION
                    SELECT target_id FROM bot_message_links JOIN ancestors ON message_id = ancestors.id
                ),
                descendants(id) AS (
                    SELECT id FROM selected UNION
                    SELECT message_id FROM bot_message_links JOIN descendants ON target_id = descendants.id
                )
             SELECT {MESSAGE_COLUMNS} FROM bot_messages m CROSS JOIN selected s
             WHERE m.id IN (SELECT id FROM ancestors UNION SELECT id FROM descendants)
             ORDER BY m.id = s.id DESC, abs(m.timeline_ms - s.timeline_ms), m.id DESC LIMIT 51"),
        ).bind(bot_id).bind(id).fetch_all(&self.pool).await?;
        if rows.is_empty() {
            return Err(Error::NotFound(format!("Bot message {id} not found")));
        }
        let truncated = rows.len() > 50;
        rows.truncate(50);
        rows.sort_by(|a, b| (a.timeline_ms, &a.message.id).cmp(&(b.timeline_ms, &b.message.id)));
        Ok(MessageContext {
            messages: rows.into_iter().map(|row| row.message).collect(),
            truncated,
        })
    }

    pub async fn list(&self, bot_id: &str) -> Result<Vec<BotMessage>> {
        Ok(sqlx::query_as(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM bot_messages m WHERE bot_id = ? ORDER BY created_at, id"
        ))
        .bind(bot_id)
        .fetch_all(&self.pool)
        .await?)
    }

    pub(crate) async fn page(
        &self,
        bot_id: &str,
        page: TimePage<'_>,
        limit: u32,
    ) -> Result<Vec<TimedMessage>> {
        let mut query = sqlx::QueryBuilder::new(format!(
            "SELECT {MESSAGE_COLUMNS} FROM bot_messages m WHERE bot_id = "
        ));
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
        Ok(sqlx::query_as(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM bot_messages m WHERE bot_id = ? AND id = ?"
        ))
        .bind(bot_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?)
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
        let mut tx = self.pool.begin().await?;
        let failed = sqlx::query_as(
            "UPDATE bot_messages SET status = 'failed'
             WHERE bot_id = ? AND id = ? RETURNING *",
        )
        .bind(&message.bot_id)
        .bind(&message.id)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM bot_todos WHERE message_id = ?")
            .bind(&message.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(failed)
    }
}

async fn link_message(
    tx: &mut sqlx::SqliteConnection,
    message: &BotMessage,
    target: &str,
) -> Result<bool> {
    let result = sqlx::query(
        "INSERT INTO bot_message_links(message_id, target_id)
        SELECT ?, id FROM bot_messages WHERE bot_id = ? AND id = ? AND id != ?",
    )
    .bind(&message.id)
    .bind(&message.bot_id)
    .bind(target)
    .bind(&message.id)
    .execute(tx)
    .await?;
    Ok(result.rows_affected() == 1)
}

pub(crate) async fn append_message<'a>(
    executor: impl sqlx::SqliteExecutor<'a>,
    bot_id: &str,
    kind: MessageKind,
    content: &str,
    link_source: Option<&str>,
) -> Result<BotMessage> {
    if content.trim().is_empty() {
        return Err(Error::Domain("message content cannot be empty".into()));
    }
    sqlx::query_as(
        "INSERT INTO bot_messages (id, bot_id, kind, content, link_source)
             SELECT ?, bot_id, ?, ?, ? FROM bots WHERE bot_id = ? RETURNING *",
    )
    .bind(format!("bmsg_{}", uuid::Uuid::now_v7()))
    .bind(kind)
    .bind(content)
    .bind(link_source)
    .bind(bot_id)
    .fetch_optional(executor)
    .await?
    .ok_or_else(|| Error::NotFound(format!("Bot {bot_id} not found")))
}

#[cfg(test)]
mod summary_tests {
    use super::*;
    #[test]
    fn summarizes_secret_labels_and_masks_after_quotes_and_uploads() {
        let content = "<user_quote speaker=\"Bot\">hi</user_quote>\n<user_uploads note=\"files\">\n<user_upload path=\"/f\">file</user_upload>\n</user_uploads>\n<user_secrets note=\"private\">\n<user_secret path=\"/private\" label=\"A &amp; B\">*******</user_secret>\n<user_secret path=\"/hidden\">ab*******cd</user_secret>\n</user_secrets>";
        assert_eq!(
            summary(MessageKind::UserInput, content, 160),
            "file, A & B, ab*******cd"
        );
        assert_eq!(
            summary(MessageKind::UserInput, &format!("{content}\nDeploy now"), 6),
            "Deploy"
        );
    }
}

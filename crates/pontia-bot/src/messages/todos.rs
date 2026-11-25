use super::{BotMessage, MESSAGE_COLUMNS, MessageService, Todo};
use pontia_core::{Error, Result};
use serde::Serialize;

#[derive(Serialize)]
pub struct TodoList {
    pub open: Vec<BotMessage>,
    pub recently_resolved: Vec<BotMessage>,
}

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TodoAction {
    Resolve,
    Mark,
}

impl MessageService {
    pub(crate) async fn todos(&self, bot_id: &str) -> Result<TodoList> {
        let mut tx = self.pool.begin().await?;
        let query = format!(
            "SELECT {MESSAGE_COLUMNS} FROM bot_messages m
            JOIN bot_todos t ON t.message_id = m.id WHERE m.bot_id = ?"
        );
        let open = sqlx::query_as(&format!(
            "{query} AND t.state = 'open' ORDER BY t.opened_at DESC, m.id DESC"
        ))
        .bind(bot_id)
        .fetch_all(&mut *tx)
        .await?;
        let recently_resolved = sqlx::query_as(&format!(
            "{query} AND t.state = 'done' ORDER BY t.resolved_at DESC, m.id DESC LIMIT 20"
        ))
        .bind(bot_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(TodoList {
            open,
            recently_resolved,
        })
    }

    pub(crate) async fn change_todo(
        &self,
        bot_id: &str,
        id: &str,
        action: TodoAction,
        actor: &str,
    ) -> Result<(BotMessage, bool, Option<Todo>)> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let query =
            format!("SELECT {MESSAGE_COLUMNS} FROM bot_messages m WHERE m.bot_id = ? AND m.id = ?");
        let mut message: BotMessage = sqlx::query_as(&query)
            .bind(bot_id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Bot message {id} not found")))?;
        if message.status != super::MessageStatus::Normal {
            return Err(Error::StateConflict(
                "Retry the failed message first".into(),
            ));
        }
        let previous = message.todo.clone();
        let open = matches!(previous, Some(Todo::Open { .. }));
        let changed = open != matches!(action, TodoAction::Mark);
        if changed {
            let update = match action {
                TodoAction::Mark => "INSERT INTO bot_todos(opened_by,message_id,state) VALUES (?,?,'open')
                    ON CONFLICT(message_id) DO UPDATE SET state='open', opened_by=excluded.opened_by,
                    opened_at=excluded.opened_at, resolved_by=NULL, resolved_at=NULL, resolved_message_id=NULL",
                TodoAction::Resolve => "UPDATE bot_todos SET state='done', resolved_by=?,
                    resolved_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE message_id=?",
            };
            sqlx::query(update)
                .bind(actor)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            message = sqlx::query_as(&query)
                .bind(bot_id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok((message, changed, previous))
    }
}

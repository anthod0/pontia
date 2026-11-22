use super::{BotMessage, MESSAGE_COLUMNS, MessageService, Todo};
use pontia_core::{Error, Result};
use serde::Serialize;

#[derive(Serialize)]
pub struct TodoList {
    pub open: Vec<BotMessage>,
    pub recently_resolved: Vec<BotMessage>,
}

#[derive(Clone, Copy)]
pub enum TodoAction {
    Resolve,
    Reopen,
}

impl MessageService {
    pub(crate) async fn todos(&self, bot_id: &str) -> Result<TodoList> {
        let mut tx = self.pool.begin().await?;
        let query = format!(
            "SELECT {MESSAGE_COLUMNS} FROM bot_messages m
            JOIN bot_todos t ON t.message_id = m.id WHERE m.bot_id = ?"
        );
        let open = sqlx::query_as(&format!(
            "{query} AND t.state = 'open' ORDER BY m.timeline_ms DESC, m.id DESC"
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
    ) -> Result<(BotMessage, bool)> {
        let update = match action {
            TodoAction::Resolve => "UPDATE bot_todos SET state = 'done', resolved_by = 'user',
                resolved_at = strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE state = 'open'",
            TodoAction::Reopen => "UPDATE bot_todos SET state = 'open', resolved_by = NULL,
                resolved_at = NULL, resolved_message_id = NULL WHERE state = 'done' AND resolved_by = 'user'",
        };
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query(&format!(
            "{update} AND message_id = (SELECT id FROM bot_messages WHERE bot_id = ? AND id = ?)"
        ))
        .bind(bot_id)
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            == 1;
        let message: BotMessage = sqlx::query_as(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM bot_messages m JOIN bot_todos t ON t.message_id = m.id WHERE m.bot_id = ? AND m.id = ?"
        )).bind(bot_id).bind(id).fetch_optional(&mut *tx).await?
            .ok_or_else(|| Error::NotFound(format!("Todo {id} not found")))?;
        if !changed
            && matches!(action, TodoAction::Reopen)
            && matches!(&message.todo, Some(Todo::Done { by, .. }) if by == "bot")
        {
            return Err(Error::StateConflict(
                "A todo closed by the Bot cannot be reopened".into(),
            ));
        }
        tx.commit().await?;
        Ok((message, changed))
    }
}

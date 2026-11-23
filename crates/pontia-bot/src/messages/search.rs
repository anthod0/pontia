use pontia_core::{Error, Result};
use serde::Serialize;

use super::{BotMessage, MESSAGE_COLUMNS, MessageService};

const MAX_QUERY_CHARACTERS: usize = 200;

#[derive(Serialize)]
pub struct SearchPage {
    pub messages: Vec<BotMessage>,
    /// The last returned message ID, when older matches remain.
    pub next_before: Option<String>,
}

impl MessageService {
    /// Messages containing `query`, newest first. ASCII letters match case-insensitively.
    pub(crate) async fn search(
        &self,
        bot_id: &str,
        query: &str,
        before: Option<&str>,
        limit: u32,
    ) -> Result<SearchPage> {
        let query = query.trim();
        if query.is_empty() {
            return Err(Error::Domain("search text cannot be empty".into()));
        }
        if query.chars().count() > MAX_QUERY_CHARACTERS {
            return Err(Error::Domain(format!(
                "search text must be at most {MAX_QUERY_CHARACTERS} characters"
            )));
        }
        if !(1..=100).contains(&limit) {
            return Err(Error::Domain("limit must be between 1 and 100".into()));
        }
        let pattern = format!(
            "%{}%",
            query
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_")
        );
        let mut sql = sqlx::QueryBuilder::new(format!(
            "SELECT {MESSAGE_COLUMNS} FROM bot_messages m WHERE m.bot_id = "
        ));
        sql.push_bind(bot_id)
            .push(" AND m.content LIKE ")
            .push_bind(pattern)
            .push(" ESCAPE '\\'");
        if let Some(before) = before {
            let anchor: Option<(i64,)> =
                sqlx::query_as("SELECT timeline_ms FROM bot_messages WHERE bot_id = ? AND id = ?")
                    .bind(bot_id)
                    .bind(before)
                    .fetch_optional(&self.pool)
                    .await?;
            let (ms,) = anchor.ok_or_else(|| Error::Domain("Invalid search cursor".into()))?;
            sql.push(" AND (m.timeline_ms, m.id) < (")
                .push_bind(ms)
                .push(", ")
                .push_bind(before)
                .push(")");
        }
        sql.push(" ORDER BY m.timeline_ms DESC, m.id DESC LIMIT ")
            .push_bind(limit + 1);
        let mut messages: Vec<BotMessage> = sql.build_query_as().fetch_all(&self.pool).await?;
        let more = messages.len() > limit as usize;
        messages.truncate(limit as usize);
        let next_before = more.then(|| messages.last().unwrap().id.clone());
        Ok(SearchPage {
            messages,
            next_before,
        })
    }
}

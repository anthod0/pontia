use std::collections::BTreeSet;

use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{MAX_RELATED, MessageService};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MessageLinks {
    pub message_id: String,
    pub related_to: BTreeSet<String>,
}

impl MessageService {
    pub(crate) async fn replace_links(&self, bot_id: &str, links: &MessageLinks) -> Result<()> {
        if !(1..=MAX_RELATED).contains(&links.related_to.len()) {
            return Err(Error::Domain(
                "provide one to five distinct related messages".into(),
            ));
        }
        let id = &links.message_id;
        let mut tx = self.pool.begin().await?;
        let (ms, content): (i64, String) = sqlx::query_as(
            "UPDATE bot_messages SET link_source = 'bot'
             WHERE bot_id = ? AND id = ? AND kind = 'user_input' AND link_source IS NOT 'quote'
             RETURNING timeline_ms, content",
        )
        .bind(bot_id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            Error::Domain(format!(
                "message {id} must be an unquoted user message in this Bot chat"
            ))
        })?;
        // Older quotes can have no stored edge, e.g. when a quoted message was deleted.
        if crate::quote::user_input(&content).1.is_some() {
            return Err(Error::Domain(format!(
                "message {id} already contains a quote"
            )));
        }
        sqlx::query("DELETE FROM bot_message_links WHERE message_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        for target in &links.related_to {
            let result = sqlx::query(
                "INSERT INTO bot_message_links(message_id, target_id)
                 SELECT ?, id FROM bot_messages WHERE bot_id = ? AND id = ? AND (timeline_ms, id) < (?, ?)",
            )
            .bind(id).bind(bot_id).bind(target).bind(ms).bind(id).execute(&mut *tx).await?;
            if result.rows_affected() != 1 {
                return Err(Error::Domain(format!(
                    "related message {target} must precede {id} in this Bot chat"
                )));
            }
        }
        tx.commit().await?;
        Ok(())
    }
}

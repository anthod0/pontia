use super::{BotRole, ConversationService};
use crate::messages::{BotMessage, SearchPage};
use pontia_agent_control::queries::TimePage;
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatQuery {
    Search {
        keyword: String,
        before: Option<String>,
        limit: u32,
    },
    Context {
        message_id: String,
        radius: u32,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum ChatResult {
    Search(SearchPage),
    Context { messages: Vec<BotMessage> },
}

impl ConversationService {
    pub async fn chat_history(&self, session_id: &str, query: ChatQuery) -> Result<ChatResult> {
        let Some((bot_id, BotRole::Retrieval)) = self.active_helper(session_id).await? else {
            return Err(Error::Conflict {
                code: "bot_session_required",
                message: "Chat history requires a deep retrieval helper Session".into(),
            });
        };
        match query {
            ChatQuery::Search {
                keyword,
                before,
                limit,
            } => Ok(ChatResult::Search(
                self.messages
                    .search(&bot_id, &keyword, before.as_deref(), limit)
                    .await?,
            )),
            ChatQuery::Context { message_id, radius } => {
                if radius > 50 {
                    return Err(Error::Domain("radius must be between 0 and 50".into()));
                }
                let anchor = self
                    .messages
                    .find(&bot_id, &message_id)
                    .await?
                    .ok_or_else(|| {
                        Error::NotFound(format!("Bot message {message_id} not found"))
                    })?;
                let before = TimePage::Before(anchor.timeline_ms, &message_id);
                let after = TimePage::After(anchor.timeline_ms, &message_id);
                let mut rows = self.messages.page(&bot_id, before, radius).await?;
                rows.reverse();
                rows.push(anchor);
                rows.extend(self.messages.page(&bot_id, after, radius).await?);
                Ok(ChatResult::Context {
                    messages: rows.into_iter().map(|row| row.message).collect(),
                })
            }
        }
    }
}

use super::{BotRole, ConversationService};
use crate::messages::{Attachment, BotMessage, SearchType};
use pontia_agent_control::queries::TimePage;
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatQuery {
    Search {
        query: String,
        #[serde(default, rename = "type")]
        kind: SearchType,
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
    Search(ChatSearchPage),
    Context { messages: Vec<BotMessage> },
}

#[derive(Serialize)]
pub struct ChatSearchPage {
    messages: Vec<ChatSearchHit>,
    next_before: Option<String>,
}

#[derive(Serialize)]
pub struct ChatSearchHit {
    id: String,
    created_at: String,
    sender: &'static str,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    attachments: Option<Vec<Attachment>>,
}

impl ConversationService {
    pub async fn chat_history(&self, session_id: &str, query: ChatQuery) -> Result<ChatResult> {
        let bot_id = if let Some(bot) = self.bot_for_session(session_id).await? {
            bot.bot_id
        } else if let Some((bot_id, BotRole::Retrieval)) = self.active_helper(session_id).await? {
            bot_id
        } else {
            return Err(Error::Conflict {
                code: "bot_session_required",
                message: "Chat history requires the current Bot or its deep retrieval helper"
                    .into(),
            });
        };
        match query {
            ChatQuery::Search {
                query,
                kind,
                before,
                limit,
            } => {
                let page = self
                    .search_messages(&bot_id, &query, kind, before.as_deref(), limit)
                    .await?;
                Ok(ChatResult::Search(ChatSearchPage {
                    messages: page
                        .messages
                        .into_iter()
                        .map(|hit| ChatSearchHit {
                            id: hit.message.id,
                            created_at: hit.message.created_at,
                            sender: match hit.message.kind {
                                crate::messages::MessageKind::UserInput => "user",
                                crate::messages::MessageKind::BotReply => "bot",
                            },
                            content: hit.message.content,
                            attachments: hit.attachments,
                        })
                        .collect(),
                    next_before: page.next_before,
                }))
            }
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

use serde::{Deserialize, Serialize};

use super::{TurnTimelineDirection, TurnTimelineService, TurnTimelineServiceError};
use crate::TurnTimelineItem;
use crate::client_contract::native_history::HistorySource;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageCategory {
    User,
    Assistant,
    ToolCall,
    ToolResult,
    Other,
}

impl MessageCategory {
    fn of(item: &TurnTimelineItem) -> Self {
        match item.item.kind.as_str() {
            "tool_call" => Self::ToolCall,
            "tool_result" => Self::ToolResult,
            _ => match item.item.role.as_str() {
                "user" => Self::User,
                "assistant" => Self::Assistant,
                _ => Self::Other,
            },
        }
    }
}

/// Turn anchors are inclusive; each page is returned in chronological order.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMessagesQuery {
    pub session_id: String,
    #[serde(default = "backward")]
    pub direction: TurnTimelineDirection,
    #[serde(default)]
    pub turn_id: Option<String>,
    /// Number of Turns scanned, including Turns with no matching messages.
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default = "default_categories")]
    pub categories: Vec<MessageCategory>,
    #[serde(default)]
    pub history_source: Option<HistorySource>,
}

fn backward() -> TurnTimelineDirection {
    TurnTimelineDirection::Backward
}
fn default_limit() -> usize {
    10
}
fn default_categories() -> Vec<MessageCategory> {
    vec![
        MessageCategory::User,
        MessageCategory::Assistant,
        MessageCategory::Other,
    ]
}

#[derive(Debug, Serialize)]
pub struct SessionMessagesPage {
    pub session_id: String,
    pub items: Vec<TurnTimelineItem>,
    pub has_more: bool,
    /// Submit this query unchanged to retain the direction, source and filters.
    pub next: Option<SessionMessagesQuery>,
    pub history_source: Option<HistorySource>,
}

impl TurnTimelineService {
    pub async fn messages(
        &self,
        query: SessionMessagesQuery,
    ) -> Result<SessionMessagesPage, TurnTimelineServiceError> {
        if !(1..=100).contains(&query.limit) || query.categories.is_empty() {
            return Err(TurnTimelineServiceError::InvalidQuery(
                "limit must be 1–100 Turns and categories must be nonempty".into(),
            ));
        }
        let mut service = self.clone().with_history_source(query.history_source);
        service.require_source = true;
        let page = service
            .page(
                query.session_id.clone(),
                query.direction,
                query.turn_id.clone(),
                query.limit,
            )
            .await?;
        let items = page
            .items
            .into_iter()
            .filter(|item| query.categories.contains(&MessageCategory::of(item)))
            .collect();
        let next = page.next_turn_id.map(|turn_id| SessionMessagesQuery {
            turn_id: Some(turn_id),
            history_source: page.history_source.or(query.history_source),
            ..query
        });
        Ok(SessionMessagesPage {
            session_id: page.session_id,
            items,
            has_more: next.is_some(),
            next,
            history_source: page.history_source,
        })
    }
}

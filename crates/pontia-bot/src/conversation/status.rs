use pontia_core::{
    Result,
    domain::{DomainEvent, EventType},
};
use serde::Serialize;

use super::{Activity, ConversationService, ConversationUpdate, NOTICE_EVENTS};
use crate::{identity::BotRole, records::Bot};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkStatus {
    Processing,
    Working {
        title: String,
    },
    /// The Bot's own Session is still running after it has answered.
    Busy,
    Idle,
}

impl ConversationService {
    pub(super) async fn await_reply(&self, bot_id: &str) -> Result<()> {
        sqlx::query("UPDATE bots SET awaiting_reply = 1 WHERE bot_id = ?")
            .bind(bot_id)
            .execute(&self.control.db())
            .await?;
        Ok(())
    }

    pub(super) async fn work_status(&self, bot: &Bot, activity: &Activity) -> Result<WorkStatus> {
        if self.processing(bot, activity).await? {
            return Ok(WorkStatus::Processing);
        }
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT session_id FROM bot_helpers WHERE bot_id = ? AND session_id IS NOT NULL",
        )
        .bind(&bot.bot_id)
        .fetch_all(&self.control.db())
        .await?;
        let latest = self
            .control
            .queries()
            .busy_session_summaries(&ids)
            .await?
            .into_iter()
            .filter(|session| {
                crate::helpers::helper_role(session.role.as_deref()) == BotRole::Helper
            })
            .max_by(|a, b| (&a.created_at, &a.session_id).cmp(&(&b.created_at, &b.session_id)));
        if let Some(title) = latest.and_then(|session| session.title) {
            return Ok(WorkStatus::Working { title });
        }
        let Some(current) = &bot.current_session_id else {
            return Ok(WorkStatus::Idle);
        };
        let busy = !self
            .control
            .queries()
            .busy_session_summaries(std::slice::from_ref(current))
            .await?
            .is_empty();
        Ok(if busy {
            WorkStatus::Busy
        } else {
            WorkStatus::Idle
        })
    }

    async fn processing(&self, bot: &Bot, activity: &Activity) -> Result<bool> {
        if !bot.awaiting_reply {
            return Ok(false);
        }
        if !activity.pending_inputs.lock().unwrap().is_empty() {
            return Ok(true);
        }
        let (Some(session), Some(position)) = (&bot.current_session_id, bot.awaiting_reply_after)
        else {
            return Ok(false);
        };
        let Some(snapshot) = self.control.queries().session_snapshot(session).await? else {
            return Ok(false);
        };
        if matches!(snapshot.state.as_str(), "exited" | "error") {
            return Ok(false);
        }
        if snapshot
            .turns
            .iter()
            .any(|turn| matches!(turn.state.as_str(), "queued" | "running"))
        {
            return Ok(true);
        }
        // Native clients can acknowledge input before publishing the next Turn.
        // Only an ending fact after delivery settles an unanswered input.
        let endings = [
            EventType::TurnCompleted,
            EventType::TurnFailed,
            EventType::TurnInterrupted,
            EventType::TurnDispatchFailed,
            EventType::SessionExited,
            EventType::SessionError,
        ]
        .map(|kind| kind.to_string());
        Ok(!self
            .control
            .events()
            .has_session_event_after(session, position, &endings)
            .await?)
    }

    pub async fn event_update(
        &self,
        bot_id: &str,
        event: &DomainEvent,
    ) -> Result<Option<ConversationUpdate>> {
        if !matches!(
            event.event_type,
            EventType::TurnCreated
                | EventType::TurnQueued
                | EventType::TurnStarted
                | EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnInterrupted
                | EventType::SessionTitleUpdated
                | EventType::InteractionBlocked
                | EventType::InteractionUnblocked
                | EventType::SessionReady
                | EventType::SessionError
                | EventType::SessionExited
        ) {
            return Ok(None);
        }
        let bot = self.records.get(bot_id).await?;
        if bot.current_session_id.as_deref() == Some(&event.session_id) {
            return Ok(Some(if NOTICE_EVENTS.contains(&event.event_type) {
                ConversationUpdate::Reset
            } else {
                ConversationUpdate::Changed
            }));
        }
        if crate::helpers::helper_bot(&self.control.db(), &event.session_id)
            .await?
            .as_deref()
            == Some(bot_id)
        {
            return Ok(Some(ConversationUpdate::Changed));
        }
        Ok(None)
    }
}

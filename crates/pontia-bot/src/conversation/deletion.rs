use pontia_agent_control::SessionView;
use pontia_core::{Error, Result};
use serde::Serialize;

use super::{APPLICATION_ID, ConversationService, ConversationUpdate};

/// Conditions that keep a Bot from being deleted; deletion never stops working Sessions or helpers.
#[derive(Debug, Default, Serialize)]
pub struct DeletionBlockers {
    /// A Bot Session is starting, working, waiting for an interaction, or receiving input.
    pub running: bool,
    /// Helpers the Bot has not closed.
    pub helpers: Vec<SessionView>,
}

impl DeletionBlockers {
    fn is_empty(&self) -> bool {
        !self.running && self.helpers.is_empty()
    }
}

impl ConversationService {
    pub async fn deletion_blockers(&self, bot_id: &str) -> Result<DeletionBlockers> {
        self.records.get(bot_id).await?;
        let pending = self.activity(bot_id).has_pending_input();
        self.blockers(bot_id, pending).await
    }

    async fn blockers(&self, bot_id: &str, pending_input: bool) -> Result<DeletionBlockers> {
        let mut running = pending_input;
        for session in self.generation_sessions(bot_id).await? {
            running |= self.session_running(&session).await?;
        }
        let helpers = crate::helpers::open_sessions(&self.control, bot_id).await?;
        Ok(DeletionBlockers { running, helpers })
    }

    /// Permanently deletes the Bot's chat, associations, and directory; Session records remain.
    pub async fn delete(&self, bot_id: &str) -> Result<()> {
        self.records.get(bot_id).await?;
        let activity = self.activity(bot_id);
        // Input being delivered rejects at once instead of waiting for the Bot to answer.
        if activity.has_pending_input() {
            return Err(still_working());
        }
        // Holding the delivery gate keeps input from starting a Bot Session during deletion,
        // and the recording gate keeps new chat messages out while they are deleted.
        let _delivery = activity.delivery.lock().await;
        let _recording = activity.recording.lock().await;
        let blockers = self.blockers(bot_id, activity.has_pending_input()).await?;
        if !blockers.is_empty() {
            return Err(blocked(&blockers));
        }
        // Each main Session is settled; end it so no runtime outlives its Bot. Its record remains.
        for session in self.generation_sessions(bot_id).await? {
            let state = self
                .control
                .queries()
                .session_snapshot(&session)
                .await?
                .ok_or_else(|| Error::NotFound(format!("Session {session} not found")))?
                .state;
            let ended = match state.as_str() {
                "exited" => true,
                "idle" => {
                    self.control
                        .session_commands()
                        .exit_if_idle(&session)
                        .await?
                }
                "error" | "interrupted" => {
                    self.control
                        .session_commands()
                        .terminate_session(&session)
                        .await?;
                    true
                }
                // The Session started working after the check above.
                _ => false,
            };
            if !ended {
                return Err(still_working());
            }
        }
        self.records.delete(bot_id).await?;
        activity.notify(ConversationUpdate::Reset);
        // Dropping the activity closes open chat streams for this Bot.
        self.activity.lock().unwrap().remove(bot_id);
        Ok(())
    }

    /// Main Sessions from every generation of this Bot, including retired ones.
    async fn generation_sessions(&self, bot_id: &str) -> Result<Vec<String>> {
        let prefix = format!("{bot_id}:");
        let mut sessions = Vec::new();
        for session in self
            .control
            .queries()
            .list_owned_sessions(APPLICATION_ID)
            .await?
        {
            if self
                .control
                .queries()
                .session_ownership(&session)
                .await?
                .is_some_and(|ownership| ownership.reference.starts_with(&prefix))
            {
                sessions.push(session);
            }
        }
        Ok(sessions)
    }

    async fn session_running(&self, session: &str) -> Result<bool> {
        let snapshot = self
            .control
            .queries()
            .session_snapshot(session)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session} not found")))?;
        Ok(match snapshot.state.as_str() {
            "exited" | "error" => false,
            "idle" | "interrupted" => {
                snapshot.has_unsettled_input
                    || snapshot
                        .turns
                        .iter()
                        .any(|turn| matches!(turn.state.as_str(), "queued" | "running"))
            }
            _ => true,
        })
    }
}

fn still_working() -> Error {
    blocked(&DeletionBlockers {
        running: true,
        ..Default::default()
    })
}

fn blocked(blockers: &DeletionBlockers) -> Error {
    let mut reasons = Vec::new();
    if blockers.running {
        reasons.push("the Bot is still working; wait until it finishes".to_owned());
    }
    if !blockers.helpers.is_empty() {
        let helpers = blockers
            .helpers
            .iter()
            .map(|helper| helper.session_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        reasons.push(format!(
            "{} helper(s) are still open ({helpers}); close them first",
            blockers.helpers.len()
        ));
    }
    Error::Conflict {
        code: "bot_delete_blocked",
        message: format!("Cannot delete this Bot: {}", reasons.join("; ")),
    }
}

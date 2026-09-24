use super::*;
use crate::SessionCommandService;

impl InboxCommandService {
    pub async fn fail_prepared_message(
        &self,
        session_id: &str,
        message_id: &str,
        reason: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE inbox_messages SET state='failed',failure_message=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE session_id=? AND message_id=? AND state IN ('resuming','pending')")
            .bind(reason).bind(session_id).bind(message_id).execute(&self.pool).await?;
        Ok(())
    }

    /// Restore an exited conversation and hold its input until the caller has
    /// committed its scheduling decision. Startup recovery fails interrupted
    /// preparation instead of launching or delivering it a second time.
    pub async fn prepare_message_once(
        &self,
        sessions: &SessionCommandService,
        message_id: &str,
        session_id: &str,
        input: String,
        metadata: Value,
    ) -> Result<InboxCommandOutcome> {
        let request = SubmitInboxMessageRequest {
            input,
            delivery_policy: "after_idle".into(),
            branch_target_turn_id: None,
            metadata,
        };
        let lock = self.scheduler.command_lock(session_id);
        let _guard = lock.lock().await;
        let outcome = self
            .enqueue_message(message_id, session_id, request, None, true)
            .await?;
        if !outcome.duplicate {
            let session = self
                .queries
                .get_session_control(session_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?;
            let result = match session.state.as_str() {
                "exited" => sessions.resume_for_input(session_id).await,
                "idle" => Ok(()),
                state => Err(Error::StateConflict(format!(
                    "Session {session_id} cannot prepare input in {state}"
                ))),
            };
            if let Err(error) = result {
                self.mark_failed(
                    message_id,
                    format!("Session recovery failed before input delivery: {error}"),
                )
                .await?;
            }
        }
        Ok(InboxCommandOutcome {
            data: json!({"inbox_message": self.get_message(session_id, message_id).await?}),
            duplicate: outcome.duplicate,
        })
    }

    pub async fn release_prepared_message(
        &self,
        session_id: &str,
        message_id: &str,
        required_runtime_instance_id: &str,
    ) -> Result<()> {
        let lock = self.scheduler.command_lock(session_id);
        let _guard = lock.lock().await;
        let message = self
            .get_message(session_id, message_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Inbox message {message_id} not found")))?;
        if !matches!(message.state.as_str(), "resuming" | "pending") {
            return Ok(());
        }
        // Never turn an in-flight or uncertain delivery back into pending.
        sqlx::query("UPDATE inbox_messages SET state='pending',required_runtime_instance_id=?,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE session_id=? AND message_id=? AND state='resuming'")
            .bind(required_runtime_instance_id).bind(session_id).bind(message_id).execute(&self.pool).await?;
        self.drain_inbox(session_id).await
    }
}

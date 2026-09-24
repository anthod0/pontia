use super::*;
use crate::SessionCommandService;

#[derive(Debug, Deserialize)]
pub struct RetryInboxMessageRequest {
    pub message_id: String,
    #[serde(default)]
    pub allow_unknown: bool,
}

impl InboxCommandService {
    pub async fn retry_message(
        &self,
        sessions: &SessionCommandService,
        session_id: &str,
        original_id: &str,
        request: RetryInboxMessageRequest,
    ) -> Result<InboxCommandOutcome> {
        let lock = self.scheduler.command_lock(session_id);
        let _guard = lock.lock().await;
        let original = self
            .get_message(session_id, original_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Inbox message {original_id} not found")))?;
        if let Some(retry) = original.retried_by_message_id {
            let message = self.get_message(session_id, &retry).await?.unwrap();
            return Ok(InboxCommandOutcome {
                data: json!({"inbox_message":message}),
                duplicate: true,
            });
        }
        if original.state != "failed" && !(original.state == "unknown" && request.allow_unknown) {
            return Err(Error::StateConflict("Retry requires a failed input, or explicit acknowledgement that unknown input may execute twice".into()));
        }
        let session = self
            .queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session_id} not found")))?;
        let resuming = session.state == "exited";
        let mut metadata = original.metadata;
        if let Some(object) = metadata.as_object_mut() {
            object.remove("inbox_message_id");
            object.remove("codex_turn_id");
        }
        let outcome = self
            .enqueue_message(
                &request.message_id,
                session_id,
                SubmitInboxMessageRequest {
                    input: original.input.summary,
                    delivery_policy: original.delivery_policy,
                    branch_target_turn_id: original.branch_target_turn_id,
                    metadata,
                },
                Some(original_id),
                resuming,
            )
            .await?;
        if resuming && !outcome.duplicate {
            match sessions.resume_for_input(session_id).await {
                Ok(()) => {
                    SqliteInboxRepository::new(self.pool.clone())
                        .return_pending(&request.message_id)
                        .await?;
                    self.drain_inbox(session_id).await?;
                }
                Err(error) => {
                    self.mark_failed(
                        &request.message_id,
                        format!("Session recovery failed before input delivery: {error}"),
                    )
                    .await?
                }
            }
        }
        Ok(InboxCommandOutcome {
            data: json!({"inbox_message":self.get_message(session_id, &request.message_id).await?.unwrap()}),
            duplicate: outcome.duplicate,
        })
    }
}

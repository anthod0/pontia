use super::*;
use crate::turns::InputIntent;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveInputDelivery {
    Accepted,
    Queued,
}

impl InboxCommandOutcome {
    pub fn live_delivery(&self) -> Result<LiveInputDelivery> {
        let message = &self.data["inbox_message"];
        let failure = || {
            message["failure_message"]
                .as_str()
                .unwrap_or("Inbox delivery failed")
                .to_owned()
        };
        match message["state"].as_str() {
            Some("pending" | "dispatching") => Ok(LiveInputDelivery::Queued),
            Some("dispatched") => Ok(LiveInputDelivery::Accepted),
            Some("unknown") => Err(Error::ControlUnknown(failure())),
            _ => Err(Error::StateConflict(failure())),
        }
    }
}

impl InboxCommandService {
    /// Delivers directly unless the active Session needs to wait for its next Turn.
    pub async fn submit_live_input(
        &self,
        session_id: &str,
        input: String,
    ) -> Result<LiveInputDelivery> {
        use std::time::Duration;
        use tokio::time::{Instant, sleep};

        let commands = self.scheduler.command_lock(session_id);
        let _command = commands.lock().await;
        let lock = self.scheduler.session_lock(session_id);
        let guard = lock.lock().await;
        let deadline = Instant::now() + Duration::from_secs(5);
        let metadata = json!({});
        loop {
            let active = SqliteTurnRepository::new(self.pool.clone())
                .active_turn(session_id)
                .await?;
            let intent = match active {
                Some(turn) => {
                    let session = self
                        .queries
                        .get_session_control(session_id)
                        .await?
                        .ok_or_else(|| {
                            Error::NotFound(format!("session {session_id} not found"))
                        })?;
                    if session.capabilities.accept_task && !session.capabilities.steer {
                        drop(guard);
                        return self.queue_live_input(session_id, input).await;
                    }
                    InputIntent::Steer {
                        turn_id: turn.turn_id,
                    }
                }
                None => InputIntent::Start,
            };
            let result = self
                .turns
                .submit_input(session_id, input.clone(), metadata.clone(), intent, None)
                .await
                .and_then(|(_, receipt)| receipt.into_result());
            match result {
                Err(Error::Conflict {
                    code: "input_busy" | "steer_target_changed",
                    ..
                }) if Instant::now() < deadline => {
                    sleep(Duration::from_millis(25)).await;
                }
                Err(Error::Conflict {
                    code: "steer_method_unavailable",
                    ..
                }) => {
                    drop(guard);
                    return self.queue_live_input(session_id, input).await;
                }
                result => return result.map(|_| LiveInputDelivery::Accepted),
            }
        }
    }

    async fn queue_live_input(&self, session_id: &str, input: String) -> Result<LiveInputDelivery> {
        let outcome = self
            .enqueue_message(
                &new_message_id().to_string(),
                session_id,
                SubmitInboxMessageRequest {
                    input,
                    delivery_policy: "after_idle".into(),
                    metadata: json!({}),
                    branch_target_turn_id: None,
                },
                None,
                false,
            )
            .await?;
        outcome.live_delivery()
    }
}

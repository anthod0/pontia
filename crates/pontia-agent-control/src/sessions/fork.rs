use super::SessionCommandService;
use crate::client_contract::NativeSessionFork;
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ForkSessionRequest {
    pub operation_id: String,
    pub turn_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForkSessionStage {
    ForkUnconfirmed,
    ForkFailed,
    SessionPending,
    SessionCreated,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ForkSessionOutcome {
    pub stage: ForkSessionStage,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ForkOperation {
    turn_id: String,
    native: Option<NativeSessionFork>,
    outcome: ForkSessionOutcome,
}

impl SessionCommandService {
    pub async fn fork_session(
        &self,
        session_id: &str,
        request: ForkSessionRequest,
    ) -> Result<ForkSessionOutcome> {
        if request.operation_id.is_empty()
            || request.operation_id.len() > 128
            || !request
                .operation_id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err(Error::Domain("Invalid fork operation identity".into()));
        }
        // Keep retries of an operation behind its first native receipt and adoption.
        let _guard = self.fork_gate.lock().await;
        let session = self
            .queries
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if !session.capabilities.fork_session {
            return Err(Error::CapabilityUnavailable(
                "Session fork is unavailable".into(),
            ));
        }
        let client = self
            .clients
            .registry()
            .get(&session.client_type)
            .and_then(|entry| entry.session.clone())
            .ok_or_else(|| Error::CapabilityUnavailable("Session fork is unavailable".into()))?;
        let path = format!("$.history_forks.\"{}\"", request.operation_id);
        let stored: Option<String> =
            sqlx::query_scalar("SELECT json_extract(metadata, ?) FROM sessions WHERE session_id=?")
                .bind(&path)
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?;
        let mut operation = if let Some(stored) = stored {
            let stored: ForkOperation = serde_json::from_str(&stored)?;
            if stored.turn_id != request.turn_id {
                return Err(Error::StateConflict(
                    "Fork operation belongs to a different Turn".into(),
                ));
            }
            if stored.native.is_none()
                || matches!(stored.outcome.stage, ForkSessionStage::SessionCreated)
            {
                return Ok(stored.outcome);
            }
            stored
        } else {
            let eligible: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM turns t JOIN native_turn_bindings n ON n.session_id=t.session_id AND n.turn_id=t.turn_id WHERE t.session_id=? AND t.turn_id=? AND t.state IN ('completed','failed','interrupted'))",
            ).bind(session_id).bind(&request.turn_id).fetch_one(&self.pool).await?;
            if !eligible {
                return Err(Error::StateConflict(
                    "Fork requires a completed native Turn boundary".into(),
                ));
            }
            let mut operation = ForkOperation {
                turn_id: request.turn_id,
                native: None,
                outcome: ForkSessionOutcome {
                    stage: ForkSessionStage::ForkUnconfirmed,
                    session_id: None,
                    error: Some("The native fork result has not been confirmed. Do not start another fork to retry this operation.".into()),
                },
            };
            // Persist uncertainty before sending. A restart cannot safely repeat a native fork.
            self.save_fork_operation(session_id, &path, &operation)
                .await?;
            match client
                .fork_session(self.event_ingest.clone(), session_id, &operation.turn_id)
                .await
            {
                Ok(native) => {
                    operation.native = Some(native);
                    operation.outcome.stage = ForkSessionStage::SessionPending;
                    operation.outcome.error = None;
                }
                Err(error) => {
                    operation.outcome.stage = if matches!(error, Error::ControlUnknown(_)) {
                        ForkSessionStage::ForkUnconfirmed
                    } else {
                        ForkSessionStage::ForkFailed
                    };
                    operation.outcome.error = Some(error.to_string());
                }
            }
            self.save_fork_operation(session_id, &path, &operation)
                .await?;
            operation
        };
        if let Some(native) = &operation.native {
            match client
                .adopt_fork(self.event_ingest.clone(), session_id, native)
                .await
            {
                Ok(session_id) => {
                    operation.outcome = ForkSessionOutcome {
                        stage: ForkSessionStage::SessionCreated,
                        session_id: Some(session_id),
                        error: None,
                    };
                }
                Err(error) => {
                    operation.outcome.error = Some(error.to_string());
                    operation.outcome.session_id =
                        crate::AgentBindingService::new(self.pool.clone())
                            .binding_for_client_session(
                                &session.client_type,
                                &native.client_session_key,
                            )
                            .await?
                            .map(|binding| binding.session_id);
                }
            }
            self.save_fork_operation(session_id, &path, &operation)
                .await?;
        }
        Ok(operation.outcome)
    }

    async fn save_fork_operation(
        &self,
        session: &str,
        path: &str,
        operation: &ForkOperation,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET metadata=json_set(metadata,?,json(?)) WHERE session_id=?")
            .bind(path)
            .bind(serde_json::to_string(operation)?)
            .bind(session)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

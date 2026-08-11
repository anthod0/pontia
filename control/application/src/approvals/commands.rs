use pontia_core::error::{Error, Result};
use serde_json::Value;
use sqlx::SqlitePool;

use super::{ApprovalCoordinator, ApprovalDecisionRequest, ApprovalWaitOutcome};

#[derive(Clone)]
pub struct ApprovalCommandService {
    pool: SqlitePool,
    coordinator: ApprovalCoordinator,
}

impl ApprovalCommandService {
    pub fn new(pool: SqlitePool, coordinator: ApprovalCoordinator) -> Self {
        Self { pool, coordinator }
    }

    pub async fn decide(
        &self,
        session_id: &str,
        request_event_id: &str,
        request: ApprovalDecisionRequest,
    ) -> Result<Value> {
        let _finalization = self.coordinator.finalization.lock().await;
        let row = sqlx::query_as::<_, (String, String)>(
            r#"SELECT session_id, payload
               FROM events
               WHERE event_id = ? AND event_type = 'approval.requested'"#,
        )
        .bind(request_event_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| {
            Error::StateConflict("Approval request is no longer actionable".to_string())
        })?;
        if row.0 != session_id {
            return Err(Error::StateConflict(
                "Approval request does not belong to the target Session".to_string(),
            ));
        }
        let payload: Value = serde_json::from_str(&row.1)?;
        let metadata =
            sqlx::query_scalar::<_, String>("SELECT metadata FROM sessions WHERE session_id = ?")
                .bind(session_id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or_else(|| {
                    Error::StateConflict("Approval request is no longer actionable".to_string())
                })?;
        let metadata: Value = serde_json::from_str(&metadata)?;
        let interaction = metadata.get("interaction");
        if interaction
            .and_then(|value| value.get("type"))
            .and_then(Value::as_str)
            != Some("approval")
            || interaction
                .and_then(|value| value.get("state"))
                .and_then(Value::as_str)
                != Some("awaiting")
            || interaction
                .and_then(|value| value.get("request_event_id"))
                .and_then(Value::as_str)
                != Some(request_event_id)
        {
            return Err(Error::StateConflict(
                "Approval request is no longer actionable".to_string(),
            ));
        }

        let outcome = match request {
            ApprovalDecisionRequest::AcceptOnce => ApprovalWaitOutcome::AcceptOnce,
            ApprovalDecisionRequest::Reject => ApprovalWaitOutcome::Reject,
            ApprovalDecisionRequest::AlwaysAllow {
                permission_suggestion,
            } => {
                let event_matches = payload
                    .get("permission_suggestions")
                    .and_then(Value::as_array)
                    .is_some_and(|suggestions| {
                        suggestions
                            .iter()
                            .any(|suggestion| suggestion == &permission_suggestion)
                    });
                if !event_matches {
                    return Err(Error::StateConflict(
                        "permission suggestion does not exactly match approval.requested"
                            .to_string(),
                    ));
                }
                ApprovalWaitOutcome::AlwaysAllow {
                    permission_suggestion,
                }
            }
        };
        self.coordinator
            .deliver_decision(request_event_id, session_id, outcome)
            .await?;
        Ok(serde_json::json!({
            "request_event_id": request_event_id,
            "delivered": true,
        }))
    }
}

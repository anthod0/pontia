use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    error::Result,
    ids::new_event_id,
};
use serde_json::{Map, Value};
use sqlx::SqlitePool;

use crate::{AgentBindingService, EventIngestService};

use super::{ApprovalCoordinator, coordinator::ApprovalAcceptScope, validation::bounded_required};

#[derive(Debug, Clone, Copy)]
enum ClaudeDecision {
    Accept,
    Reject,
}

impl ClaudeDecision {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "accept" => Some(Self::Accept),
            "reject" => Some(Self::Reject),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClaudeDecisionSource {
    Config,
    Hook,
    UserPermanent,
    UserTemporary,
    UserAbort,
    Other,
}

impl From<&str> for ClaudeDecisionSource {
    fn from(value: &str) -> Self {
        match value {
            "config" => Self::Config,
            "hook" => Self::Hook,
            "user_permanent" => Self::UserPermanent,
            "user_temporary" => Self::UserTemporary,
            "user_abort" => Self::UserAbort,
            _ => Self::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClaudeToolDecisionObservation {
    pub client_session_id: String,
    pub prompt_id: String,
    pub tool_name: String,
    pub tool_use_id: Option<String>,
    pub decision: String,
    pub decision_source: String,
}

#[derive(Clone)]
pub struct ApprovalObservationService {
    pool: SqlitePool,
    coordinator: ApprovalCoordinator,
}

impl ApprovalObservationService {
    pub fn new(pool: SqlitePool, coordinator: ApprovalCoordinator) -> Self {
        Self { pool, coordinator }
    }

    pub async fn observe_claude_tool_decision(
        &self,
        observation: ClaudeToolDecisionObservation,
    ) -> Result<bool> {
        let client_session_id = bounded_required("session.id", &observation.client_session_id)?;
        let prompt_id = bounded_required("prompt.id", &observation.prompt_id)?;
        let tool_name = bounded_required("tool_name", &observation.tool_name)?;
        let Some(decision) =
            ClaudeDecision::parse(bounded_required("decision", &observation.decision)?)
        else {
            return Ok(false);
        };
        let decision_source =
            ClaudeDecisionSource::from(bounded_required("source", &observation.decision_source)?);
        let tool_use_id = observation
            .tool_use_id
            .as_deref()
            .map(|value| bounded_required("tool_use_id", value))
            .transpose()?;

        let _finalization = self.coordinator.finalization.lock().await;
        let Some(context) = AgentBindingService::new(self.pool.clone())
            .current_turn_for_client_session("claude", client_session_id)
            .await?
        else {
            return Ok(false);
        };

        let unresolved = sqlx::query_as::<_, (String, String)>(
            r#"SELECT requested.event_id, requested.payload
               FROM events requested
               WHERE requested.session_id = ?
                 AND requested.turn_id = ?
                 AND requested.event_type = 'approval.requested'
                 AND NOT EXISTS (
                     SELECT 1
                     FROM events final
                     WHERE final.session_id = requested.session_id
                       AND final.turn_id = requested.turn_id
                       AND final.event_type IN (
                           'approval.accepted',
                           'approval.rejected',
                           'approval.cancelled'
                       )
                       AND json_extract(final.payload, '$.request_event_id') =
                           requested.event_id
                 )"#,
        )
        .bind(&context.session_id)
        .bind(&context.turn_id)
        .fetch_all(&self.pool)
        .await?;
        let [(request_event_id, requested_payload)] = unresolved.as_slice() else {
            return Ok(false);
        };
        let requested_payload: Value = serde_json::from_str(requested_payload)?;
        if requested_payload
            .get("client_session_id")
            .and_then(Value::as_str)
            != Some(client_session_id)
            || requested_payload.get("prompt_id").and_then(Value::as_str) != Some(prompt_id)
            || requested_payload.get("tool_name").and_then(Value::as_str) != Some(tool_name)
        {
            return Ok(false);
        }

        let (event_type, accepted_scope) = match decision {
            ClaudeDecision::Accept => {
                let scope = match decision_source {
                    ClaudeDecisionSource::UserTemporary => ApprovalAcceptScope::Once,
                    ClaudeDecisionSource::UserPermanent => ApprovalAcceptScope::Always,
                    ClaudeDecisionSource::Config => ApprovalAcceptScope::Unknown,
                    ClaudeDecisionSource::Hook => self
                        .coordinator
                        .hook_accept_scope(request_event_id)
                        .await
                        .unwrap_or(ApprovalAcceptScope::Unknown),
                    _ => return Ok(false),
                };
                (EventType::ApprovalAccepted, Some(scope))
            }
            ClaudeDecision::Reject if decision_source == ClaudeDecisionSource::UserAbort => {
                (EventType::ApprovalCancelled, None)
            }
            ClaudeDecision::Reject => (EventType::ApprovalRejected, None),
        };

        let mut payload = Map::new();
        payload.insert(
            "request_event_id".to_string(),
            Value::String(request_event_id.clone()),
        );
        payload.insert(
            "client_session_id".to_string(),
            Value::String(client_session_id.to_string()),
        );
        payload.insert(
            "prompt_id".to_string(),
            Value::String(prompt_id.to_string()),
        );
        payload.insert(
            "tool_name".to_string(),
            Value::String(tool_name.to_string()),
        );
        if let Some(tool_use_id) = tool_use_id {
            payload.insert(
                "tool_use_id".to_string(),
                Value::String(tool_use_id.to_string()),
            );
        }
        if let Some(scope) = accepted_scope {
            payload.insert(
                "scope".to_string(),
                Value::String(scope.as_str().to_string()),
            );
        }

        EventIngestService::new(self.pool.clone())
            .ingest_reported_event(ReportedEvent::new(
                new_event_id().to_string(),
                context.session_id,
                Some(context.turn_id),
                EventSource::AgentClient,
                "claude".to_string(),
                event_type,
                Value::Object(payload),
            ))
            .await?;
        self.coordinator.resolve_request(request_event_id).await;
        Ok(true)
    }
}

use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    error::{Error, Result},
    ids::new_event_id,
};
use serde_json::{Map, Value};
use sqlx::SqlitePool;

use crate::{AgentBindingService, EventIngestService};

use super::{
    ApprovalCoordinator, ApprovalRegistrationRequest, MAX_PERMISSION_SUGGESTIONS, PendingApproval,
    validation::{bounded_required, valid_permission_suggestion},
};

#[derive(Clone)]
pub struct ApprovalRegistrationService {
    pool: SqlitePool,
    coordinator: ApprovalCoordinator,
}

impl ApprovalRegistrationService {
    pub fn new(pool: SqlitePool, coordinator: ApprovalCoordinator) -> Self {
        Self { pool, coordinator }
    }

    pub async fn register(
        &self,
        request: ApprovalRegistrationRequest,
    ) -> Result<Option<PendingApproval>> {
        let client_session_key = bounded_required("session_id", &request.session_id)?;
        let tool_name = bounded_required("tool_name", &request.tool_name)?;
        let prompt_id = request
            .prompt_id
            .as_deref()
            .map(|value| bounded_required("prompt_id", value))
            .transpose()?;
        if !request.tool_input.is_object() {
            return Err(Error::Domain(
                "tool_input must be a JSON object".to_string(),
            ));
        }
        if !request.hook_input.is_object() {
            return Err(Error::Domain(
                "hook_input must be a JSON object".to_string(),
            ));
        }

        let Some(context) = AgentBindingService::new(self.pool.clone())
            .current_turn_for_client_session("claude", client_session_key)
            .await?
        else {
            return Ok(None);
        };

        let permission_suggestions = request
            .permission_suggestions
            .iter()
            .take(MAX_PERMISSION_SUGGESTIONS)
            .filter(|suggestion| valid_permission_suggestion(suggestion))
            .cloned()
            .collect::<Vec<_>>();
        let request_event_id = new_event_id().to_string();
        let mut payload = Map::new();
        payload.insert(
            "client_session_id".to_string(),
            Value::String(client_session_key.to_string()),
        );
        if let Some(prompt_id) = prompt_id {
            payload.insert(
                "prompt_id".to_string(),
                Value::String(prompt_id.to_string()),
            );
        }
        payload.insert(
            "tool_name".to_string(),
            Value::String(tool_name.to_string()),
        );
        payload.insert(
            "permission_suggestions".to_string(),
            Value::Array(permission_suggestions.clone()),
        );

        let receiver = self
            .coordinator
            .register(
                request_event_id.clone(),
                context.session_id.clone(),
                context.turn_id.clone(),
                request.hook_input,
                request.permission_suggestions,
            )
            .await?;
        let event = ReportedEvent::new(
            request_event_id.clone(),
            context.session_id.clone(),
            Some(context.turn_id.clone()),
            EventSource::AgentClient,
            "claude".to_string(),
            EventType::ApprovalRequested,
            Value::Object(payload),
        );
        if let Err(error) = EventIngestService::new(self.pool.clone())
            .ingest_reported_event(event)
            .await
        {
            self.coordinator.remove(&request_event_id).await;
            return Err(error);
        }
        watch_terminal_projection(
            self.pool.clone(),
            self.coordinator.clone(),
            request_event_id.clone(),
            context.session_id.clone(),
            context.turn_id.clone(),
        );

        Ok(Some(PendingApproval {
            request_event_id,
            session_id: context.session_id,
            turn_id: context.turn_id,
            receiver,
        }))
    }
}

fn watch_terminal_projection(
    pool: SqlitePool,
    coordinator: ApprovalCoordinator,
    request_event_id: String,
    session_id: String,
    turn_id: String,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let states = sqlx::query_as::<_, (String, String)>(
                r#"SELECT s.state, t.state
                   FROM sessions s
                   JOIN turns t ON t.session_id = s.session_id
                   WHERE s.session_id = ? AND t.turn_id = ?"#,
            )
            .bind(&session_id)
            .bind(&turn_id)
            .fetch_optional(&pool)
            .await;
            let Ok(Some((session_state, turn_state))) = states else {
                continue;
            };
            if matches!(session_state.as_str(), "exited" | "error")
                || matches!(
                    turn_state.as_str(),
                    "completed" | "failed" | "interrupted" | "abandoned"
                )
            {
                coordinator.resolve_request(&request_event_id).await;
                break;
            }
        }
    });
}

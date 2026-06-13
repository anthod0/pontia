use std::str::FromStr;

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    application::{AppState, EventIngestService},
    domain::{DomainEvent, EventSource, EventType},
    error::Error,
};

const MAX_EVENT_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
pub struct InternalEventRequest {
    event_id: String,
    session_id: String,
    turn_id: Option<String>,
    source: String,
    client_type: String,
    #[serde(rename = "type")]
    event_type: String,
    time: String,
    seq: Option<i64>,
    payload: Value,
}

#[derive(Debug, Serialize)]
pub struct InternalEventResponse {
    accepted: bool,
    duplicate: bool,
    event_id: String,
    session_id: String,
    turn_id: Option<String>,
    state_version: i64,
    warnings: Vec<String>,
}

pub async fn post_event(
    State(state): State<AppState>,
    request: Result<Json<InternalEventRequest>, JsonRejection>,
) -> Result<Json<InternalEventResponse>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let event = request.into_domain_event()?;
    ensure_agent_client_ready_references_existing_session(&state, &event).await?;
    let service = EventIngestService::new(state.db());

    if event.event_type == EventType::SessionMessageUpdated {
        let state_version: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
                .bind(&event.session_id)
                .fetch_one(&state.db())
                .await
                .map_err(Error::from)?;
        state.volatile_events().publish(event.clone());
        return Ok(Json(InternalEventResponse {
            accepted: true,
            duplicate: false,
            event_id: event.event_id,
            session_id: event.session_id,
            turn_id: event.turn_id,
            state_version,
            warnings: Vec::new(),
        }));
    }

    let warnings = service.sequence_warnings(&event).await?;
    let result = service.ingest_event(event.clone()).await?;
    let warnings = if result.duplicate {
        Vec::new()
    } else {
        warnings
    };

    if !warnings.is_empty() {
        service.record_warnings(&event, &warnings).await?;
    }

    Ok(Json(InternalEventResponse {
        accepted: result.accepted,
        duplicate: result.duplicate,
        event_id: result.event_id,
        session_id: result.session_id,
        turn_id: result.turn_id,
        state_version: result.state_version,
        warnings,
    }))
}

async fn ensure_agent_client_ready_references_existing_session(
    state: &AppState,
    event: &DomainEvent,
) -> Result<(), ApiError> {
    if event.event_type != EventType::SessionReady || event.source != EventSource::AgentClient {
        return Ok(());
    }

    let exists: i64 =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id = ?)")
            .bind(&event.session_id)
            .fetch_one(&state.db())
            .await
            .map_err(Error::from)?;
    if exists == 0 {
        return Err(ApiError::invalid_request(format!(
            "session.ready from agent_client references unknown session {}",
            event.session_id
        )));
    }

    Ok(())
}

impl InternalEventRequest {
    fn into_domain_event(self) -> Result<DomainEvent, ApiError> {
        let source = EventSource::from_str(&self.source)
            .map_err(|err| ApiError::invalid_request(err.to_string()))?;
        let event_type = EventType::from_str(&self.event_type)
            .map_err(|err| ApiError::invalid_request(err.to_string()))?;

        if event_type.requires_turn_id() && self.turn_id.is_none() {
            return Err(ApiError::invalid_request(format!(
                "event {event_type} requires turn_id"
            )));
        }

        if !self.payload.is_object() {
            return Err(ApiError::invalid_request("payload must be a JSON object"));
        }

        if event_type == EventType::SessionContextUsageUpdated {
            validate_context_usage_payload(&self.payload)?;
        }

        if event_type == EventType::SessionReady && source == EventSource::AgentClient {
            let runtime_instance_id = self
                .payload
                .get("runtime_instance_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if runtime_instance_id.trim().is_empty() {
                return Err(ApiError::invalid_request(
                    "session.ready from agent_client requires payload.runtime_instance_id",
                ));
            }
            if self.client_type == "pi" {
                let client_session_key = self
                    .payload
                    .get("client_session_key")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if client_session_key.trim().is_empty() {
                    return Err(ApiError::invalid_request(
                        "pi session.ready from agent_client requires payload.client_session_key",
                    ));
                }
            }
        }

        let payload_size = serde_json::to_vec(&self.payload)
            .map_err(Error::from)?
            .len();
        if payload_size > MAX_EVENT_PAYLOAD_BYTES {
            return Err(ApiError::invalid_request(format!(
                "payload exceeds maximum size of {MAX_EVENT_PAYLOAD_BYTES} bytes"
            )));
        }

        let occurred_at = OffsetDateTime::parse(&self.time, &Rfc3339)
            .map_err(|err| ApiError::invalid_request(format!("invalid time: {err}")))?;

        Ok(DomainEvent {
            event_id: self.event_id,
            session_id: self.session_id,
            turn_id: self.turn_id,
            source,
            client_type: self.client_type,
            event_type,
            occurred_at,
            seq: self.seq,
            payload: self.payload,
        })
    }
}

fn validate_context_usage_payload(payload: &Value) -> Result<(), ApiError> {
    let usage = payload
        .get("context_usage")
        .and_then(Value::as_object)
        .ok_or_else(|| ApiError::invalid_request("payload.context_usage must be a JSON object"))?;

    for field in [
        "used_tokens",
        "max_tokens",
        "remaining_tokens",
        "input_tokens",
        "output_tokens",
        "cache_tokens",
    ] {
        if let Some(value) = usage.get(field)
            && !value.is_null()
            && value.as_u64().is_none()
        {
            return Err(ApiError::invalid_request(format!(
                "payload.context_usage.{field} must be a non-negative integer"
            )));
        }
    }

    if let Some(value) = usage.get("usage_ratio")
        && !value.is_null()
    {
        let ratio = value.as_f64().ok_or_else(|| {
            ApiError::invalid_request("payload.context_usage.usage_ratio must be between 0 and 1")
        })?;
        if !(0.0..=1.0).contains(&ratio) {
            return Err(ApiError::invalid_request(
                "payload.context_usage.usage_ratio must be between 0 and 1",
            ));
        }
    }

    if usage.contains_key("model") {
        return Err(ApiError::invalid_request(
            "payload.context_usage.model is not supported; use payload.model",
        ));
    }

    if let Some(value) = usage.get("confidence")
        && !value.is_null()
    {
        match value.as_str() {
            Some("exact" | "estimated" | "unknown") => {}
            _ => {
                return Err(ApiError::invalid_request(
                    "payload.context_usage.confidence must be exact, estimated, or unknown",
                ));
            }
        }
    }

    if let Some(value) = payload.get("model")
        && !value.is_null()
        && value.as_str().is_none()
    {
        return Err(ApiError::invalid_request(
            "payload.model must be a string or null",
        ));
    }

    Ok(())
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
        }
    }
}

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::Domain(message) | Error::StateConflict(message) => Self {
                status: StatusCode::CONFLICT,
                code: "state_conflict",
                message,
            },
            Error::NotFound(message) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message,
            },
            other => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: other.to_string(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({
            "error": {
                "code": self.code,
                "message": self.message,
            }
        }));
        (self.status, body).into_response()
    }
}

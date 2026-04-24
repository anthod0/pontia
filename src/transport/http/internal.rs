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
    let service = EventIngestService::new(state.db);
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
            Error::Domain(message) => Self {
                status: StatusCode::CONFLICT,
                code: "state_conflict",
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

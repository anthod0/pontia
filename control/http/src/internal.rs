use std::str::FromStr;

use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use pontia_application::{
    AgentBindingService, AppState, CurrentTurnClaimRequest, CurrentTurnClaimService,
    EventIngestService, InternalEventValidationService, RuntimeBindingUpsertRequest,
    RuntimeBindingUpsertService,
};
use pontia_core::{
    domain::{DomainEvent, EventSource, EventType, MAX_TURN_OUTPUT_SUMMARY_CHARS, ReportedEvent},
    error::Error,
};
use pontia_dag::DagRunResultService;

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
    turn_index: Option<Value>,
    timeline_boundary: Option<Value>,
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

#[derive(Debug, Deserialize)]
pub struct AgentBindingQuery {
    client_type: String,
    client_session_key: String,
}

pub async fn get_agent_binding(
    State(state): State<AppState>,
    Query(query): Query<AgentBindingQuery>,
) -> Result<Json<Value>, ApiError> {
    let client_type = required_query_param("client_type", &query.client_type)?;
    let client_session_key = required_query_param("client_session_key", &query.client_session_key)?;
    let binding = AgentBindingService::new(state.db())
        .binding_for_client_session(client_type, client_session_key)
        .await?
        .ok_or_else(|| Error::NotFound("agent binding not found".to_string()))?;

    Ok(Json(json!({ "data": { "binding": binding } })))
}

pub async fn get_agent_binding_session_context(
    State(state): State<AppState>,
    Query(query): Query<AgentBindingQuery>,
) -> Result<Json<Value>, ApiError> {
    let client_type = required_query_param("client_type", &query.client_type)?;
    let client_session_key = required_query_param("client_session_key", &query.client_session_key)?;
    let session_context = AgentBindingService::new(state.db())
        .session_context_for_client_session(client_type, client_session_key)
        .await?
        .ok_or_else(|| {
            Error::NotFound("session context for agent binding not found".to_string())
        })?;

    Ok(Json(
        json!({ "data": { "session_context": session_context } }),
    ))
}

pub async fn get_agent_binding_current_turn(
    State(state): State<AppState>,
    Query(query): Query<AgentBindingQuery>,
) -> Result<Json<Value>, ApiError> {
    let client_type = required_query_param("client_type", &query.client_type)?;
    let client_session_key = required_query_param("client_session_key", &query.client_session_key)?;
    let current_turn = AgentBindingService::new(state.db())
        .current_turn_for_client_session(client_type, client_session_key)
        .await?
        .ok_or_else(|| Error::NotFound("active turn for agent binding not found".to_string()))?;

    Ok(Json(json!({ "data": { "current_turn": current_turn } })))
}

pub async fn upsert_runtime_binding(
    State(state): State<AppState>,
    request: Result<Json<RuntimeBindingUpsertRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let response = RuntimeBindingUpsertService::new(state.db())
        .upsert(request)
        .await?;
    Ok(Json(response))
}

pub async fn claim_current_turn(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    request: Result<Json<CurrentTurnClaimRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let current_turn = CurrentTurnClaimService::new(state.db())
        .claim(&session_id, request)
        .await?;
    Ok(Json(json!({ "data": { "current_turn": current_turn } })))
}

pub async fn post_event(
    State(state): State<AppState>,
    request: Result<Json<InternalEventRequest>, JsonRejection>,
) -> Result<Json<InternalEventResponse>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let reported_event = request.into_reported_event()?;
    let event = DomainEvent::from(reported_event.clone());
    InternalEventValidationService::new()
        .validate(&event)
        .map_err(domain_error_as_invalid_request)?;
    let service = EventIngestService::new(state.db());
    service
        .ensure_confirmed_event_matches_session_boundary(&event)
        .await
        .map_err(domain_error_as_invalid_request)?;

    if event.event_type == EventType::SessionMessageUpdated {
        let state_version = service.volatile_state_version(&event.session_id).await?;
        state
            .volatile_events()
            .publish_debounced_session_message_updated(event.clone());
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
    let result = service.ingest_event(reported_event).await?;
    if !result.duplicate {
        DagRunResultService::with_graph(state.db(), state.graph())
            .sync_from_turn_event(&event)
            .await?;
    }
    let warnings = if result.duplicate {
        Vec::new()
    } else {
        warnings
    };

    for warning in &warnings {
        tracing::warn!(
            code = "event_ingest_sequence_anomaly",
            event_id = %event.event_id,
            session_id = %event.session_id,
            turn_id = ?event.turn_id,
            seq = event.seq,
            warning,
            "event ingest sequence anomaly"
        );
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

fn required_query_param<'a>(name: &str, value: &'a str) -> Result<&'a str, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::invalid_request(format!(
            "{name} query parameter is required"
        )));
    }
    Ok(value)
}

fn domain_error_as_invalid_request(error: Error) -> ApiError {
    match error {
        Error::Domain(message) => ApiError::invalid_request(message),
        other => ApiError::from(other),
    }
}

impl InternalEventRequest {
    fn into_reported_event(self) -> Result<ReportedEvent, ApiError> {
        let source = EventSource::from_str(&self.source)
            .map_err(|err| ApiError::invalid_request(err.to_string()))?;
        let event_type = EventType::from_str(&self.event_type)
            .map_err(|err| ApiError::invalid_request(err.to_string()))?;

        if event_type.requires_turn_id() && self.turn_id.is_none() {
            return Err(ApiError::invalid_request(format!(
                "event {event_type} requires turn_id"
            )));
        }
        if self.turn_index.is_some() {
            return Err(ApiError::invalid_request(
                "turn_index is Pontia-owned and cannot be reported",
            ));
        }
        if self.timeline_boundary.is_some() {
            return Err(ApiError::invalid_request(
                "timeline_boundary is Pontia-owned and cannot be reported",
            ));
        }

        if !self.payload.is_object() {
            return Err(ApiError::invalid_request("payload must be a JSON object"));
        }

        let mut payload = self.payload;
        if event_type == EventType::TurnOutput {
            truncate_turn_output(&mut payload);
        }
        if event_type == EventType::SessionContextUsageUpdated {
            validate_context_usage_payload(&payload)?;
        }

        let payload_size = serde_json::to_vec(&payload).map_err(Error::from)?.len();
        if payload_size > MAX_EVENT_PAYLOAD_BYTES {
            return Err(ApiError::invalid_request(format!(
                "payload exceeds maximum size of {MAX_EVENT_PAYLOAD_BYTES} bytes"
            )));
        }

        let occurred_at = OffsetDateTime::parse(&self.time, &Rfc3339)
            .map_err(|err| ApiError::invalid_request(format!("invalid time: {err}")))?;

        Ok(ReportedEvent {
            event_id: self.event_id,
            session_id: self.session_id,
            turn_id: self.turn_id,
            source,
            client_type: self.client_type,
            event_type,
            occurred_at,
            seq: self.seq,
            payload,
        })
    }
}

fn truncate_turn_output(payload: &mut Value) {
    let Some(Value::String(summary)) = payload.pointer_mut("/output/summary") else {
        return;
    };
    if summary.chars().count() <= MAX_TURN_OUTPUT_SUMMARY_CHARS {
        return;
    }
    *summary = summary
        .chars()
        .take(MAX_TURN_OUTPUT_SUMMARY_CHARS)
        .collect();
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

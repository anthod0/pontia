use std::str::FromStr;

use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use pontia_application::{
    AppState, EventIngestService, EventReportNormalizer, InternalEventValidationService,
    ReportedFact,
};
use pontia_core::{
    domain::{DomainEvent, EventType, MAX_TURN_OUTPUT_SUMMARY_CHARS},
    error::Error,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::response::ApiError;

const MAX_EVENT_PAYLOAD_BYTES: usize = 64 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InternalEventRequest {
    session_id: String,
    turn_id: Option<String>,
    #[serde(rename = "type")]
    fact_type: String,
    #[serde(alias = "payload")]
    data: Value,
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
    let fact = request.into_reported_fact()?;
    let mut reported_event = EventReportNormalizer::new(state.db())
        .normalize(fact)
        .await
        .map_err(|error| ApiError::invalid_request(error.to_string()))?;
    if reported_event.event_type == EventType::TurnOutput {
        truncate_turn_output(&mut reported_event.payload);
    }
    if reported_event.event_type == EventType::SessionContextUsageUpdated {
        validate_context_usage_payload(&reported_event.payload)?;
    }
    let payload_size = serde_json::to_vec(&reported_event.payload)
        .map_err(Error::from)?
        .len();
    if payload_size > MAX_EVENT_PAYLOAD_BYTES {
        return Err(ApiError::invalid_request(format!(
            "payload exceeds maximum size of {MAX_EVENT_PAYLOAD_BYTES} bytes"
        )));
    }
    let event = DomainEvent::from(reported_event.clone());
    InternalEventValidationService::new()
        .validate(&event)
        .map_err(domain_error_as_invalid_request)?;
    let service = EventIngestService::new(state.db()).with_agent_events(state.agent_events());
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

    let terminal_candidate = reported_event.clone();
    let result = service.ingest_confirmed_event(reported_event).await?;
    state
        .approvals()
        .resolve_terminal_event(&terminal_candidate)
        .await;

    Ok(Json(InternalEventResponse {
        accepted: result.accepted,
        duplicate: result.duplicate,
        event_id: result.event_id,
        session_id: result.session_id,
        turn_id: result.turn_id,
        state_version: result.state_version,
        warnings: Vec::new(),
    }))
}

impl InternalEventRequest {
    fn into_reported_fact(self) -> Result<ReportedFact, ApiError> {
        let fact_type = EventType::from_str(&self.fact_type)
            .map_err(|err| ApiError::invalid_request(err.to_string()))?;
        if !self.data.is_object() {
            return Err(ApiError::invalid_request("data must be a JSON object"));
        }
        Ok(ReportedFact {
            session_id: self.session_id,
            turn_id: self.turn_id,
            fact_type,
            data: self.data,
        })
    }
}

fn domain_error_as_invalid_request(error: Error) -> ApiError {
    match error {
        Error::Domain(message) => ApiError::invalid_request(message),
        other => ApiError::from(other),
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

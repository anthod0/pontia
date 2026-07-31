use std::str::FromStr;

use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use pontia_application::{
    AgentBindingService, AppState, ApprovalRegistrationRequest, ApprovalRegistrationService,
    BranchReplayService, CurrentTurnClaimRequest, CurrentTurnClaimService, EventIngestService,
    EventReportNormalizer, InternalEventValidationService, ReportedFact,
    ResolveBranchReplayRequest, RuntimeBindingUpsertRequest, RuntimeBindingUpsertService,
    SessionCommandService,
};
use pontia_core::{
    domain::{DomainEvent, EventType, MAX_TURN_OUTPUT_SUMMARY_CHARS},
    error::Error,
};
use pontia_workflow::{SubmitWorkflowNodeRequest, WorkflowScheduler};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowSubmissionRequest {
    session_id: String,
    runtime_instance_id: String,
    output: String,
    content: String,
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

pub async fn post_claude_permission_request(
    State(state): State<AppState>,
    request: Result<Json<ApprovalRegistrationRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let Some(pending) = ApprovalRegistrationService::new(state.db(), state.approvals())
        .register(request)
        .await?
    else {
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let request_event_id = pending.request_event_id.clone();
    let outcome = pending.wait().await;
    Ok(Json(json!({
        "data": {
            "result": outcome.response_value(),
            "request_event_id": request_event_id,
        }
    }))
    .into_response())
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

pub async fn resolve_branch_replay(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<ResolveBranchReplayRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    authenticate_branch_replay(&state, &headers)?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let replay = BranchReplayService::new(state.db())
        .resolve_command(request)
        .await?;
    Ok(Json(json!({ "data": { "branch_replay": replay } })))
}

pub async fn submit_workflow_output(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<WorkflowSubmissionRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    authenticate_internal_token(
        &state,
        &headers,
        "Internal Workflow API token is not configured",
    )?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let scheduler = WorkflowScheduler::new(
        state.db(),
        SessionCommandService::new(state.db()),
        state.agent_events(),
        pontia_config::pontia_home_dir(),
    );
    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: request.session_id,
            runtime_instance_id: request.runtime_instance_id,
            output: request.output,
            content: request.content,
        })
        .await
        .map_err(ApiError::from_workflow)?;
    Ok(Json(json!({ "data": { "submitted": true } })))
}

fn authenticate_branch_replay(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    authenticate_internal_token(
        state,
        headers,
        "Internal branch replay token is not configured",
    )
}

pub(crate) fn authenticate_internal_token(
    state: &AppState,
    headers: &HeaderMap,
    not_configured_message: &'static str,
) -> Result<(), ApiError> {
    let expected = state
        .external_api_token()
        .ok_or_else(|| ApiError::authentication_failed(not_configured_message))?;
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);
    if authorized {
        Ok(())
    } else {
        Err(ApiError::authentication_failed(
            "missing or invalid bearer token",
        ))
    }
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
    pub(crate) fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            message: message.into(),
        }
    }

    fn authentication_failed(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "authentication_failed",
            message: message.into(),
        }
    }

    fn from_workflow(error: pontia_workflow::Error) -> Self {
        use pontia_workflow::Error as WorkflowError;

        match error {
            WorkflowError::Pontia(error) => Self::from(error),
            WorkflowError::WorkflowNotFound(workflow_id) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: format!("workflow {workflow_id} not found"),
            },
            WorkflowError::NodeForSessionNotFound(session_id) => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                message: format!("session {session_id} is not bound to a workflow Agent Node"),
            },
            WorkflowError::InvalidHandoffFileName(message) => {
                Self::invalid_request(format!("invalid Handoff file name: {message}"))
            }
            WorkflowError::WorkflowNotRunning { .. }
            | WorkflowError::RuntimeMismatch { .. }
            | WorkflowError::OutputMismatch { .. } => Self {
                status: StatusCode::CONFLICT,
                code: "state_conflict",
                message: error.to_string(),
            },
            WorkflowError::RootNodeNotFound(_)
            | WorkflowError::MissingCreatedSessionId
            | WorkflowError::Io(_)
            | WorkflowError::Json(_) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: error.to_string(),
            },
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
            Error::CapabilityUnavailable(message) => Self {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                code: "capability_unavailable",
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

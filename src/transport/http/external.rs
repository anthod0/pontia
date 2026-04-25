use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    application::{
        AppState, ArtifactContentService, CreateSessionRequest, ExternalQueryService,
        RuntimeControlService, SessionCommandService, SubmitTurnRequest, TurnCommandService,
    },
    error::Error,
};

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    data: Option<T>,
    meta: Value,
    error: Option<ApiErrorBody>,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    code: &'static str,
    message: String,
}

pub async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = SessionCommandService::new(state.db);
    let outcome = service.create_session(request, idempotency_key).await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    let sessions = service.list_sessions().await?;
    Ok(ok(json!({ "sessions": sessions })))
}

pub async fn get_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    let session = service
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("session {session_id} not found")))?;
    Ok(ok(json!({ "session": session })))
}

pub async fn submit_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<SubmitTurnRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = TurnCommandService::new(state.db);
    let outcome = service
        .submit_turn(&session_id, request, idempotency_key)
        .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn interrupt_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = RuntimeControlService::new(state.db);
    let outcome = service
        .interrupt_current_turn(&session_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn terminate_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = RuntimeControlService::new(state.db);
    let outcome = service
        .terminate_session(&session_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn restart_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = RuntimeControlService::new(state.db);
    let outcome = service
        .restart_session(&session_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn interrupt_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = RuntimeControlService::new(state.db);
    let outcome = service
        .interrupt_turn(&session_id, &turn_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn list_turns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    let turns = service.list_turns(&session_id).await?;
    Ok(ok(json!({ "turns": turns })))
}

pub async fn get_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    let turn = service
        .get_turn(&session_id, &turn_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("turn {turn_id} not found")))?;
    Ok(ok(json!({ "turn": turn })))
}

pub async fn list_session_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    let events = service.list_session_events(&session_id).await?;
    Ok(ok(json!({ "events": events })))
}

pub async fn list_turn_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    service
        .get_turn(&session_id, &turn_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("turn {turn_id} not found")))?;
    let events = service.list_turn_events(&session_id, &turn_id).await?;
    Ok(ok(json!({ "events": events })))
}

pub async fn list_artifacts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    let artifacts = service.list_artifacts(&session_id).await?;
    Ok(ok(json!({ "artifacts": artifacts })))
}

pub async fn get_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(artifact_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    let artifact = service
        .get_artifact(&artifact_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("artifact {artifact_id} not found")))?;
    Ok(ok(json!({ "artifact": artifact })))
}

pub async fn get_artifact_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(artifact_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ArtifactContentService::new(state.db);
    let content = service.read_content(&artifact_id).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        content.bytes,
    )
        .into_response())
}

async fn ensure_session_exists(
    service: &ExternalQueryService,
    session_id: &str,
) -> Result<(), ExternalApiError> {
    service
        .get_session(session_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("session {session_id} not found")))?;
    Ok(())
}

fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<(), ExternalApiError> {
    let Some(expected) = &state.external_api_token else {
        return Err(ExternalApiError::authentication_failed(
            "external API token is not configured",
        ));
    };

    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);

    if authorized {
        Ok(())
    } else {
        Err(ExternalApiError::authentication_failed(
            "missing or invalid bearer token",
        ))
    }
}

fn ok(data: Value) -> Json<ApiResponse<Value>> {
    Json(ApiResponse {
        data: Some(data),
        meta: json!({}),
        error: None,
    })
}

#[derive(Debug)]
pub struct ExternalApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ExternalApiError {
    fn authentication_failed(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "authentication_failed",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    fn state_conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "state_conflict",
            message: message.into(),
        }
    }

    fn capability_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "capability_unavailable",
            message: message.into(),
        }
    }
}

impl From<Error> for ExternalApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::StateConflict(message) => Self::state_conflict(message),
            Error::CapabilityUnavailable(message) => Self::capability_unavailable(message),
            Error::NotFound(message) => Self::not_found(message),
            Error::Domain(message) => Self {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_request",
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

impl IntoResponse for ExternalApiError {
    fn into_response(self) -> Response {
        let body = Json(ApiResponse::<Value> {
            data: None,
            meta: json!({}),
            error: Some(ApiErrorBody {
                code: self.code,
                message: self.message,
            }),
        });
        (self.status, body).into_response()
    }
}

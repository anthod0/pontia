use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use crate::application::{
    AppState, CreateSessionRequest, ExternalQueryService, RuntimeControlService,
    SessionCommandService, UpdateSessionRequest,
};

use super::common::{ApiResponse, ExternalApiError, authenticate, idempotency_key, ok};

pub async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = SessionCommandService::new(state.db());
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
    let service = ExternalQueryService::new(state.db());
    let sessions = service.list_sessions().await?;
    Ok(ok(json!({ "sessions": sessions })))
}

pub async fn update_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionRequest>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = SessionCommandService::new(state.db());
    let data = service.update_session(&session_id, request).await?;
    Ok(ok(data))
}

pub async fn get_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db());
    let session = service
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("session {session_id} not found")))?;
    Ok(ok(json!({ "session": session })))
}

pub async fn interrupt_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = RuntimeControlService::new(state.db());
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
    let idempotency_key = idempotency_key(&headers);
    let service = RuntimeControlService::new(state.db());
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
    let idempotency_key = idempotency_key(&headers);
    let service = RuntimeControlService::new(state.db());
    let outcome = service
        .restart_session(&session_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn resume_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = RuntimeControlService::new(state.db());
    let outcome = service.resume_session(&session_id, idempotency_key).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

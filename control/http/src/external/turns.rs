use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use pontia_application::{AppState, ExternalQueryService, RuntimeControlService};

use super::common::{
    ApiResponse, ExternalApiError, authenticate, ensure_session_exists, idempotent, ok,
};

pub async fn interrupt_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = RuntimeControlService::new(state.db());
    let operation = format!("interrupt_turn:{session_id}:{turn_id}");
    let outcome = idempotent(&state, &headers, operation, || async move {
        Ok(service.interrupt_turn(&session_id, &turn_id).await?.data)
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn list_turns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db());
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
    let service = ExternalQueryService::new(state.db());
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
    let service = ExternalQueryService::new(state.db());
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
    let service = ExternalQueryService::new(state.db());
    ensure_session_exists(&service, &session_id).await?;
    service
        .get_turn(&session_id, &turn_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("turn {turn_id} not found")))?;
    let events = service.list_turn_events(&session_id, &turn_id).await?;
    Ok(ok(json!({ "events": events })))
}

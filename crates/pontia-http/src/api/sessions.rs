use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use pontia_application::{AppState, CreateSessionRequest, UpdateSessionRequest};

use super::{
    authentication::authenticate,
    idempotency::idempotent,
    response::{ApiError, ApiResponse, ok},
};

pub async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let outcome = idempotent(&state, &headers, "create_session", || async move {
        Ok(service.create_session(request).await?.data)
    })
    .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn open_codex_tui(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    state
        .session_commands()
        .open_client_interface(&session_id)
        .await?;
    Ok(ok(
        json!({"session":state.queries().get_session(&session_id).await?}),
    ))
}

#[derive(Debug, Deserialize)]
pub struct ListSessionsQuery {
    #[serde(default)]
    include_archived: bool,
    limit: Option<u32>,
    #[serde(default)]
    include_pinned: bool,
}

pub async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.queries();
    let sessions = service
        .list_sessions(query.include_archived, query.limit, query.include_pinned)
        .await?;
    Ok(ok(json!({ "sessions": sessions })))
}

pub async fn update_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateSessionRequest>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let data = service.update_session(&session_id, request).await?;
    Ok(ok(data))
}

pub async fn pin_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let data = service.pin_session(&session_id).await?;
    Ok(ok(data))
}

pub async fn unpin_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let data = service.unpin_session(&session_id).await?;
    Ok(ok(data))
}

pub async fn archive_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let data = service.archive_session(&session_id).await?;
    Ok(ok(data))
}

pub async fn unarchive_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let data = service.unarchive_session(&session_id).await?;
    Ok(ok(data))
}

pub async fn get_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.queries();
    let session = service
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("session {session_id} not found")))?;
    Ok(ok(json!({ "session": session })))
}

pub async fn interrupt_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.turn_commands();
    let operation = format!("interrupt_current:{session_id}");
    let outcome = idempotent(&state, &headers, operation, || async move {
        Ok(service.interrupt_current_turn(&session_id).await?.data)
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn terminate_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let operation = format!("terminate_session:{session_id}");
    let outcome = idempotent(&state, &headers, operation, || async move {
        Ok(service.terminate_session(&session_id).await?.data)
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn restart_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let pontia_home = state.pontia_home().to_path_buf();
    let operation = format!("restart_session:{session_id}");
    let outcome = idempotent(&state, &headers, operation, || async move {
        Ok(service
            .restart_session(&session_id, &pontia_home)
            .await?
            .data)
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn resume_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    let pontia_home = state.pontia_home().to_path_buf();
    let operation = format!("resume_session:{session_id}");
    let outcome = idempotent(&state, &headers, operation, || async move {
        Ok(service
            .resume_session(&session_id, &pontia_home)
            .await?
            .data)
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn list_session_models(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    Ok(ok(serde_json::to_value(
        service.list_session_models(&session_id).await?,
    )
    .map_err(pontia_core::Error::from)?))
}

pub async fn set_session_model(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<pontia_application::sessions::SetSessionModelRequest>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.session_commands();
    service.set_session_model(&session_id, request).await?;
    Ok((StatusCode::ACCEPTED, ok(json!({"accepted":true}))).into_response())
}

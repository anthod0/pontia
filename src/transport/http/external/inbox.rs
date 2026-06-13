use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use crate::application::{AppState, InboxCommandService, SubmitInboxMessageRequest};

use super::common::{ApiResponse, ExternalApiError, authenticate, idempotency_key, ok};

pub async fn submit_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<SubmitInboxMessageRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = InboxCommandService::new(state.db());
    let outcome = service
        .submit_message(&session_id, request, idempotency_key)
        .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn list_inbox_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = InboxCommandService::new(state.db());
    let messages = service.list_messages(&session_id).await?;
    Ok(ok(json!({ "inbox_messages": messages })))
}

pub async fn get_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = InboxCommandService::new(state.db());
    let message = service
        .get_message(&session_id, &message_id)
        .await?
        .ok_or_else(|| {
            ExternalApiError::not_found(format!("inbox message {message_id} not found"))
        })?;
    Ok(ok(json!({ "inbox_message": message })))
}

pub async fn cancel_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = InboxCommandService::new(state.db());
    let outcome = service.cancel_message(&session_id, &message_id).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn dismiss_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = InboxCommandService::new(state.db());
    let outcome = service.dismiss_message(&session_id, &message_id).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

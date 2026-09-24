use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use pontia_application::{AppState, SubmitInboxMessageRequest};

use super::{
    authentication::authenticate,
    response::{ApiError, ApiResponse, ok},
};

pub async fn submit_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<SubmitInboxMessageRequest>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.inbox_commands();
    let outcome = match headers.get("Idempotency-Key") {
        Some(key) => {
            let key = key
                .to_str()
                .map_err(|_| ApiError::invalid_request("Invalid Idempotency-Key"))?;
            if key.is_empty() || key.len() > 256 {
                return Err(ApiError::invalid_request("Invalid Idempotency-Key"));
            }
            service
                .submit_message_once(&format!("msg_{session_id}:{key}"), &session_id, request)
                .await?
        }
        None => service.submit_message(&session_id, request).await?,
    };
    Ok((
        if outcome.duplicate {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        ok(outcome.data),
    )
        .into_response())
}

pub async fn put_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(request): Json<SubmitInboxMessageRequest>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let outcome = state
        .inbox_commands()
        .submit_message_once(&message_id, &session_id, request)
        .await?;
    Ok((
        if outcome.duplicate {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        ok(outcome.data),
    )
        .into_response())
}

pub async fn retry_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
    Json(request): Json<pontia_application::RetryInboxMessageRequest>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let outcome = state
        .inbox_commands()
        .retry_message(&state.session_commands(), &session_id, &message_id, request)
        .await?;
    Ok((
        if outcome.duplicate {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        ok(outcome.data),
    )
        .into_response())
}

pub async fn list_inbox_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.inbox_commands();
    let messages = service.list_messages(&session_id).await?;
    Ok(ok(json!({ "inbox_messages": messages })))
}

pub async fn get_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.inbox_commands();
    let message = service
        .get_message(&session_id, &message_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("inbox message {message_id} not found")))?;
    Ok(ok(json!({ "inbox_message": message })))
}

pub async fn cancel_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.inbox_commands();
    let outcome = service.cancel_message(&session_id, &message_id).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn dismiss_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.inbox_commands();
    let outcome = service.dismiss_message(&session_id, &message_id).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

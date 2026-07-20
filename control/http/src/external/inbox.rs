use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use pontia_application::{AppState, InboxCommandService, SubmitInboxMessageRequest};

use super::common::{ApiResponse, ExternalApiError, authenticate, idempotent, ok};

pub async fn submit_inbox_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<SubmitInboxMessageRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = InboxCommandService::new(state.db());
    let operation = format!("submit_inbox_message:{session_id}");
    let action_session_id = session_id.clone();
    let outcome = idempotent(&state, &headers, operation, || async move {
        Ok(service
            .submit_message(&action_session_id, request)
            .await?
            .data)
    })
    .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    let data = if outcome.duplicate {
        let message_id = outcome.data["inbox_message"]["message_id"]
            .as_str()
            .map(str::to_owned);
        if let Some(message_id) = message_id {
            let service = InboxCommandService::new(state.db());
            match service.get_message(&session_id, &message_id).await? {
                Some(message) => json!({ "inbox_message": message }),
                None => outcome.data,
            }
        } else {
            outcome.data
        }
    } else {
        outcome.data
    };
    Ok((status, ok(data)).into_response())
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

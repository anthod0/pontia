use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use pontia_agent_control::{InputPayload, SendMode};
use pontia_application::AppState;
use pontia_core::error::Error;

use super::{
    idempotency::idempotent,
    response::{ApiError, ApiResponse, ok},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendMessageRequest {
    input: String,
    #[serde(default)]
    mode: SendMode,
    #[serde(default)]
    metadata: Value,
}

pub async fn send_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    request: Result<Json<SendMessageRequest>, JsonRejection>,
) -> Result<Response, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    let operation = format!("send_message:{session_id}");
    let service = state.control().inbox_commands();
    let outcome = idempotent(&state, &headers, operation, || async move {
        let input = InputPayload::new(request.input).with_metadata(request.metadata);
        let delivery = service
            .send(&session_id, input, request.mode)
            .await
            .map_err(Error::from)?;
        Ok(json!({ "delivery": delivery }))
    })
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

pub async fn list_queue(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let messages = state
        .control()
        .inbox_commands()
        .list_messages(&session_id)
        .await?;
    Ok(ok(json!({ "messages": messages })))
}

pub async fn cancel_queued_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, message_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    let operation = format!("cancel_queued_message:{session_id}:{message_id}");
    let service = state.control().inbox_commands();
    let outcome = idempotent(&state, &headers, operation, || async move {
        let message = service.cancel_message(&session_id, &message_id).await?;
        Ok(json!({ "message": message }))
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

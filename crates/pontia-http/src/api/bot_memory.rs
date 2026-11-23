use axum::{
    Json,
    extract::{Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use pontia_bot::memory::{Error, ListMemories, MemoryRequest};
use serde_json::{Value, json};

use super::response::{ApiError, ApiResponse, ok};
use crate::HttpState;

pub async fn memory_command(
    State(state): State<HttpState>,
    headers: HeaderMap,
    request: Result<Json<MemoryRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    super::bot_context::authorize_tool(&state, &headers, &request.session_id).await?;
    let response = state
        .memory_commands()
        .execute(request)
        .await
        .map_err(|error| match error {
            Error::InvalidInput(message) => ApiError::invalid_request(message),
            Error::Forbidden => {
                ApiError::custom(StatusCode::FORBIDDEN, "forbidden", error.to_string())
            }
            Error::NotFound => ApiError::not_found(error.to_string()),
            _ => ApiError::custom(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                error.to_string(),
            ),
        })?;
    Ok(ok(json!(response)))
}

pub async fn list_memories(
    State(state): State<HttpState>,
    Query(query): Query<ListMemories>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let page = state
        .memories()
        .list(query)
        .await
        .map_err(|error| match error {
            Error::InvalidInput(message) => ApiError::invalid_request(message),
            _ => ApiError::custom(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                error.to_string(),
            ),
        })?;
    Ok(ok(json!(page)))
}

pub async fn get_user_file(
    State(state): State<HttpState>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let user = state.user_service().read().await.map_err(|error| {
        ApiError::custom(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            error.to_string(),
        )
    })?;
    Ok(ok(json!({ "user": user })))
}

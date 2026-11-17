use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
};
use pontia_bot::memory::{Error, MemoryRequest};
use serde_json::{Value, json};

use super::response::{ApiError, ApiResponse, ok};
use crate::HttpState;

pub async fn memory_command(
    State(state): State<HttpState>,
    request: Result<Json<MemoryRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
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

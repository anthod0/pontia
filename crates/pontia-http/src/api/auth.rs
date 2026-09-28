use axum::Json;
use serde_json::json;

use super::response::{ApiError, ApiResponse, ok};

pub async fn validate_auth() -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    Ok(ok(json!({ "authenticated": true })))
}

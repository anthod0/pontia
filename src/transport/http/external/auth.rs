use axum::{Json, extract::State, http::HeaderMap};
use serde_json::json;

use crate::application::AppState;

use super::common::{ApiResponse, ExternalApiError, authenticate, ok};

pub async fn validate_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<serde_json::Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    Ok(ok(json!({ "authenticated": true })))
}

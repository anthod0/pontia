use axum::{Json, extract::State, http::HeaderMap};
use serde_json::json;

use pontia_application::AppState;

use super::{
    authentication::authenticate,
    response::{ApiError, ApiResponse, ok},
};

pub async fn validate_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiError> {
    authenticate(&state, &headers)?;
    Ok(ok(json!({ "authenticated": true })))
}

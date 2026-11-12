use super::response::{ApiError, ApiResponse, ok};
use axum::{Json, extract::State};
use pontia_application::AppState;
use serde_json::{Value, json};

pub async fn update_blockers(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let blockers = state.control().queries().update_blockers().await?;
    Ok(ok(json!({ "blockers": blockers })))
}

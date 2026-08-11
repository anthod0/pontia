use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};
use pontia_application::{AppState, BranchReplayService, ResolveBranchReplayRequest};
use serde_json::{Value, json};

use super::{authentication::authenticate_internal_token, response::ApiError};

pub async fn resolve_branch_replay(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<ResolveBranchReplayRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    authenticate_internal_token(
        &state,
        &headers,
        "Internal branch replay token is not configured",
    )?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let replay = BranchReplayService::new(state.db())
        .resolve_command(request)
        .await?;
    Ok(Json(json!({ "data": { "branch_replay": replay } })))
}

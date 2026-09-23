use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde_json::{Value, json};

use pontia_application::{AppState, WorkspaceGitStatusService};

use super::{
    authentication::authenticate,
    response::{ApiError, ApiResponse, ok},
};

pub async fn get_workspace_git_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = state.queries();
    let git_status = service
        .get_workspace_git_status(&workspace_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("workspace {workspace_id} not found")))?;
    Ok(ok(json!({ "git_status": git_status })))
}

pub async fn refresh_workspace_git_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let service = WorkspaceGitStatusService::new(state.db(), state.git_refresh());
    let git_status = service.refresh_workspace_git_status(&workspace_id).await?;
    Ok(ok(json!({ "git_status": git_status })))
}

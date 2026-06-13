use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde_json::{Value, json};

use crate::application::{AppState, ExternalQueryService, WorkspaceGitStatusService};

use super::common::{ApiResponse, ExternalApiError, authenticate, ok};

pub async fn get_workspace_git_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    let git_status = service
        .get_workspace_git_status(&workspace_id)
        .await?
        .ok_or_else(|| {
            ExternalApiError::not_found(format!("workspace {workspace_id} not found"))
        })?;
    Ok(ok(json!({ "git_status": git_status })))
}

pub async fn refresh_workspace_git_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = WorkspaceGitStatusService::new(state.db);
    let git_status = service.refresh_workspace_git_status(&workspace_id).await?;
    Ok(ok(json!({ "git_status": git_status })))
}

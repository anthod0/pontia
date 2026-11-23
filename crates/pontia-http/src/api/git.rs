use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};

use pontia_application::{
    AppState,
    git::{ChangeGroup, WorkspaceGitService},
};

use super::response::{ApiError, ApiResponse, ok};

pub async fn get_workspace_git_changes(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let changes = WorkspaceGitService::new(state.db())
        .changes(&workspace_id)
        .await?;
    Ok(ok(json!({ "changes": changes })))
}

#[derive(Deserialize)]
pub struct GitDiffQuery {
    path: String,
    orig_path: Option<String>,
    group: ChangeGroup,
}

pub async fn get_workspace_git_diff(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<GitDiffQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let diff = WorkspaceGitService::new(state.db())
        .diff(
            &workspace_id,
            &query.path,
            query.orig_path.as_deref(),
            query.group,
        )
        .await?;
    Ok(ok(json!({ "diff": diff })))
}

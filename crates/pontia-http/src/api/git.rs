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
    let changes = WorkspaceGitService::new(state.db(), state.workspace_browser())
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
    let diff = WorkspaceGitService::new(state.db(), state.workspace_browser())
        .diff(
            &workspace_id,
            &query.path,
            query.orig_path.as_deref(),
            query.group,
        )
        .await?;
    Ok(ok(json!({ "diff": diff })))
}

pub async fn get_workspace_git_branches(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let branches = WorkspaceGitService::new(state.db(), state.workspace_browser())
        .branches(&workspace_id)
        .await?;
    Ok(ok(json!({ "branches": branches })))
}

#[derive(Deserialize)]
pub struct GitCommitsQuery {
    #[serde(rename = "ref")]
    ref_name: String,
    #[serde(default)]
    skip: usize,
    branch: Option<String>,
}

pub async fn get_workspace_git_commits(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
    Query(query): Query<GitCommitsQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let commits = WorkspaceGitService::new(state.db(), state.workspace_browser())
        .commits(
            &workspace_id,
            &query.ref_name,
            query.skip,
            query.branch.as_deref(),
        )
        .await?;
    Ok(ok(json!({ "commits": commits })))
}

pub async fn get_workspace_git_commit(
    State(state): State<AppState>,
    Path((workspace_id, sha)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let commit = WorkspaceGitService::new(state.db(), state.workspace_browser())
        .commit(&workspace_id, &sha)
        .await?;
    Ok(ok(json!({ "commit": commit })))
}

#[derive(Deserialize)]
pub struct GitCommitDiffQuery {
    path: String,
    orig_path: Option<String>,
}

pub async fn get_workspace_git_commit_diff(
    State(state): State<AppState>,
    Path((workspace_id, sha)): Path<(String, String)>,
    Query(query): Query<GitCommitDiffQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let diff = WorkspaceGitService::new(state.db(), state.workspace_browser())
        .commit_diff(&workspace_id, &sha, &query.path, query.orig_path.as_deref())
        .await?;
    Ok(ok(json!({ "diff": diff })))
}

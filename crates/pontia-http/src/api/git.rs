use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{future::Future, time::Duration};

use pontia_application::{
    AppState,
    git::{ChangeGroup, WorkspaceGitService},
};

use super::response::{ApiError, ApiResponse, ok};

const GIT_REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

pub(super) async fn with_git_deadline<T, F>(request: F) -> Result<T, ApiError>
where
    F: Future<Output = pontia_core::error::Result<T>>,
{
    tokio::time::timeout(GIT_REQUEST_TIMEOUT, request)
        .await
        .map_err(|_| {
            ApiError::custom(
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                "request_timeout",
                "Git request timed out",
            )
        })?
        .map_err(Into::into)
}

pub async fn get_workspace_git_changes(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service =
        WorkspaceGitService::new(state.db(), state.workspace_browser(), state.git_refresh());
    let changes = with_git_deadline(service.changes(&workspace_id)).await?;
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
    let service =
        WorkspaceGitService::new(state.db(), state.workspace_browser(), state.git_refresh());
    let diff = with_git_deadline(service.diff(
        &workspace_id,
        &query.path,
        query.orig_path.as_deref(),
        query.group,
    ))
    .await?;
    Ok(ok(json!({ "diff": diff })))
}

pub async fn get_workspace_git_branches(
    State(state): State<AppState>,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service =
        WorkspaceGitService::new(state.db(), state.workspace_browser(), state.git_refresh());
    let branches = with_git_deadline(service.branches(&workspace_id)).await?;
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
    let service =
        WorkspaceGitService::new(state.db(), state.workspace_browser(), state.git_refresh());
    let commits = with_git_deadline(service.commits(
        &workspace_id,
        &query.ref_name,
        query.skip,
        query.branch.as_deref(),
    ))
    .await?;
    Ok(ok(json!({ "commits": commits })))
}

pub async fn get_workspace_git_commit(
    State(state): State<AppState>,
    Path((workspace_id, sha)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service =
        WorkspaceGitService::new(state.db(), state.workspace_browser(), state.git_refresh());
    let commit = with_git_deadline(service.commit(&workspace_id, &sha)).await?;
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
    let service =
        WorkspaceGitService::new(state.db(), state.workspace_browser(), state.git_refresh());
    let diff = with_git_deadline(service.commit_diff(
        &workspace_id,
        &sha,
        &query.path,
        query.orig_path.as_deref(),
    ))
    .await?;
    Ok(ok(json!({ "diff": diff })))
}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn git_request_deadline_returns_gateway_timeout() {
        let result = with_git_deadline(async {
            std::future::pending::<()>().await;
            Ok(())
        })
        .await;

        assert_eq!(
            result.expect_err("deadline").into_response().status(),
            axum::http::StatusCode::GATEWAY_TIMEOUT
        );
    }
}

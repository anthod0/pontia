use axum::{
    Json,
    extract::{Path, State},
};
use pontia_application::{
    AppState,
    git::{GitWrite, PathSelection, WorkspaceGitService},
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    git::git_deadline,
    response::{ApiError, ApiResponse, ok},
};

type Response = Result<Json<ApiResponse<Value>>, ApiError>;

async fn write(state: AppState, id: String, operation: GitWrite) -> Response {
    let service =
        WorkspaceGitService::new(state.db(), state.workspace_browser(), state.git_refresh());
    let timeout =
        std::time::Duration::from_secs(if matches!(operation, GitWrite::Pull | GitWrite::Push) {
            75
        } else {
            12
        });
    let result = git_deadline(timeout, service.write(&id, operation)).await?;
    Ok(ok(json!(result)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiscardRequest {
    paths: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommitRequest {
    message: String,
    #[serde(default)]
    stage_all: bool,
}

pub(crate) async fn stage(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PathSelection>,
) -> Response {
    write(state, id, GitWrite::Stage(body)).await
}

pub(crate) async fn unstage(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<PathSelection>,
) -> Response {
    write(state, id, GitWrite::Unstage(body)).await
}

pub(crate) async fn discard(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<DiscardRequest>,
) -> Response {
    write(state, id, GitWrite::Discard(body.paths)).await
}

pub(crate) async fn commit(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CommitRequest>,
) -> Response {
    write(
        state,
        id,
        GitWrite::Commit {
            message: body.message,
            stage_all: body.stage_all,
        },
    )
    .await
}

pub(crate) async fn pull(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    write(state, id, GitWrite::Pull).await
}

pub(crate) async fn push(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    write(state, id, GitWrite::Push).await
}

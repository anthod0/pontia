use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use pontia_application::{
    AppState, ExternalQueryService, RegisterWorkspaceRequest, RenameWorkspaceRequest,
    WorkspaceBrowserService,
};

use super::common::{ApiResponse, ExternalApiError, authenticate, ok};

#[derive(Debug, Deserialize)]
pub struct WorkspaceEntriesQuery {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Deserialize)]
pub struct FilePickerQuery {
    #[serde(default, alias = "q")]
    query: String,
    limit: Option<usize>,
}

pub async fn list_workspaces(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db());
    let workspaces = service.list_workspaces().await?;
    Ok(ok(json!({ "workspaces": workspaces })))
}

pub async fn get_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db());
    let workspace = service.get_workspace(&workspace_id).await?.ok_or_else(|| {
        ExternalApiError::not_found(format!("workspace {workspace_id} not found"))
    })?;
    Ok(ok(json!({ "workspace": workspace })))
}

pub async fn rename_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
    Json(request): Json<RenameWorkspaceRequest>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = WorkspaceBrowserService::new(state.db(), state.workspace_browser());
    let workspace = service.rename_workspace(&workspace_id, request).await?;
    Ok(ok(json!({ "workspace": workspace })))
}

pub async fn delete_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = WorkspaceBrowserService::new(state.db(), state.workspace_browser());
    let workspace = service.delete_workspace(&workspace_id).await?;
    Ok(ok(json!({ "workspace": workspace })))
}

pub async fn list_workspace_roots(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = WorkspaceBrowserService::new(state.db(), state.workspace_browser());
    let roots = service.list_roots().await;
    Ok(ok(json!({ "roots": roots })))
}

pub async fn list_workspace_root_entries(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(root_id): Path<String>,
    Query(query): Query<WorkspaceEntriesQuery>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = WorkspaceBrowserService::new(state.db(), state.workspace_browser());
    let listing = service.list_entries(&root_id, &query.path).await?;
    Ok(ok(json!({
        "root_id": listing.root_id,
        "path": listing.path,
        "canonical_path": listing.canonical_path,
        "parent_path": listing.parent_path,
        "entries": listing.entries,
        "warnings": listing.warnings,
    })))
}

pub async fn pick_workspace_files(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workspace_id): Path<String>,
    Query(query): Query<FilePickerQuery>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = WorkspaceBrowserService::with_file_picker(
        state.db(),
        state.workspace_browser(),
        state.file_picker(),
    );
    let result = service
        .pick_files(&workspace_id, &query.query, query.limit)
        .await?;
    Ok(ok(json!({
        "files": result.files,
        "truncated": result.truncated,
        "warnings": result.warnings,
    })))
}

pub async fn register_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RegisterWorkspaceRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = WorkspaceBrowserService::new(state.db(), state.workspace_browser());
    let workspace = service.register_workspace(request).await?;
    Ok((StatusCode::CREATED, ok(json!({ "workspace": workspace }))).into_response())
}

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use crate::application::{AppState, DagSchedulerService, ExternalQueryService};

use super::common::{ApiResponse, ExternalApiError, authenticate, ok};

pub async fn get_task_dag(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::with_graph(state.db(), state.graph());
    ensure_task_exists(&service, &task_id).await?;
    let dag = service.get_task_dag(&task_id).await?;
    Ok(ok(json!({ "dag": dag })))
}

pub async fn list_task_work_items(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::with_graph(state.db(), state.graph());
    ensure_task_exists(&service, &task_id).await?;
    let work_items = service.list_work_items(&task_id).await?;
    Ok(ok(json!({ "work_items": work_items })))
}

pub async fn list_task_work_item_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::with_graph(state.db(), state.graph());
    ensure_task_exists(&service, &task_id).await?;
    let runs = service.list_work_item_runs(&task_id).await?;
    Ok(ok(json!({ "runs": runs })))
}

pub async fn list_task_signals(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::with_graph(state.db(), state.graph());
    ensure_task_exists(&service, &task_id).await?;
    let signals = service.list_dag_signals(&task_id).await?;
    Ok(ok(json!({ "signals": signals })))
}

pub async fn scheduler_tick(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    // DAG task development is currently frozen. Keep scheduler tick behavior intact
    // for existing callers/tests and future revival, but avoid extending this path
    // while focusing on session-first Web UI and bidirectional session control.
    authenticate(&state, &headers)?;
    let query = ExternalQueryService::with_graph(state.db(), state.graph());
    ensure_task_exists(&query, &task_id).await?;
    let scheduler = DagSchedulerService::with_graph(state.db(), state.graph())
        .schedule_task(&task_id)
        .await?;
    Ok((StatusCode::OK, ok(json!({ "scheduler": scheduler }))).into_response())
}

async fn ensure_task_exists(
    service: &ExternalQueryService,
    task_id: &str,
) -> Result<(), ExternalApiError> {
    service
        .get_task(task_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("task {task_id} not found")))?;
    Ok(())
}

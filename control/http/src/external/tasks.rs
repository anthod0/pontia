use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use pontia_application::{AppState, ExternalQueryService, TaskCommandService};

use super::common::{ApiResponse, ExternalApiError, authenticate, idempotency_key, ok};

pub async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<Value>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    Ok((
        StatusCode::GONE,
        Json(json!({
            "data": null,
            "meta": {},
            "error": {
                "code": "removed",
                "message": "direct task creation is not supported"
            }
        })),
    )
        .into_response())
}

pub async fn interrupt_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::new(state.db());
    let outcome = service.interrupt_task(&task_id, idempotency_key).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn cancel_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::new(state.db());
    let outcome = service.cancel_task(&task_id, idempotency_key).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn list_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db());
    let tasks = service.list_tasks().await?;
    Ok(ok(json!({ "tasks": tasks })))
}

pub async fn get_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db());
    let task = service
        .get_task(&task_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("task {task_id} not found")))?;
    Ok(ok(json!({ "task": task })))
}

pub async fn list_task_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db());
    service
        .get_task(&task_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("task {task_id} not found")))?;
    let events = service.list_task_events(&task_id).await?;
    Ok(ok(json!({ "events": events })))
}

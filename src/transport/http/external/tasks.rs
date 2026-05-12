use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use crate::application::{
    AppState, ConfirmTaskWorkspaceRequest, CreateTaskRequest, ExternalQueryService,
    GraphProjectionService, HumanSignalRequest, SubmitPlannerInputRequest, TaskCommandService,
};

use super::common::{ApiResponse, ExternalApiError, authenticate, idempotency_key, ok};

pub async fn create_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateTaskRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::with_runtime(state.db, state.planner, state.graph);
    let outcome = service.create_task(request, idempotency_key).await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn confirm_task_workspace(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(request): Json<ConfirmTaskWorkspaceRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::with_runtime(state.db, state.planner, state.graph);
    let outcome = service
        .confirm_workspace(&task_id, request, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn submit_planner_input(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(request): Json<SubmitPlannerInputRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::with_runtime(state.db, state.planner, state.graph);
    let outcome = service
        .submit_planner_input(&task_id, request, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn pause_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::new(state.db);
    let outcome = service.pause_task(&task_id, idempotency_key).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn resume_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::new(state.db);
    let outcome = service.resume_task(&task_id, idempotency_key).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn create_human_signal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
    Json(request): Json<HumanSignalRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::new(state.db);
    let outcome = service
        .create_human_signal(&task_id, request, idempotency_key)
        .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn interrupt_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::new(state.db);
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
    let service = TaskCommandService::new(state.db);
    let outcome = service.cancel_task(&task_id, idempotency_key).await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn list_tasks(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    let tasks = service.list_tasks().await?;
    Ok(ok(json!({ "tasks": tasks })))
}

pub async fn get_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
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
    let service = ExternalQueryService::new(state.db);
    service
        .get_task(&task_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("task {task_id} not found")))?;
    let events = service.list_task_events(&task_id).await?;
    Ok(ok(json!({ "events": events })))
}

pub async fn get_task_provenance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(task_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    ExternalQueryService::new(state.db.clone())
        .get_task(&task_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("task {task_id} not found")))?;
    let provenance = GraphProjectionService::new(state.db, state.graph)
        .task_provenance(&task_id)
        .await?;
    Ok(ok(json!(provenance)))
}

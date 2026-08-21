use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use pontia_application::AppState;
use pontia_core::Error as CoreError;
use pontia_workflow::{Error as WorkflowError, WorkflowControlService, WorkflowQueryService};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    authentication::authenticate,
    idempotency::idempotent,
    response::{ApiResponse, ExternalApiError, ok},
};

#[derive(Debug, Deserialize)]
pub struct ListWorkflowsQuery {
    limit: Option<String>,
}

pub async fn list_workflows(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ListWorkflowsQuery>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let limit = parse_limit(query.limit.as_deref())?;
    let workflows = WorkflowQueryService::new(state.db())
        .list_workflows(limit)
        .await
        .map_err(map_workflow_error)?;
    Ok(ok(json!({ "workflows": workflows })))
}

pub async fn get_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let workflow = WorkflowQueryService::new(state.db())
        .get_workflow(&workflow_id)
        .await
        .map_err(map_workflow_error)?
        .ok_or_else(|| ExternalApiError::not_found(format!("workflow {workflow_id} not found")))?;
    Ok(ok(json!({ "workflow": workflow })))
}

pub async fn pause_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    control_workflow(state, headers, workflow_id, true).await
}

pub async fn resume_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    control_workflow(state, headers, workflow_id, false).await
}

async fn control_workflow(
    state: AppState,
    headers: HeaderMap,
    workflow_id: String,
    pause: bool,
) -> Result<Response, ExternalApiError> {
    let action = if pause { "pause" } else { "resume" };
    let operation = format!("{action}_workflow:{workflow_id}");
    let action_state = state.clone();
    let action_workflow_id = workflow_id.clone();
    let outcome = idempotent(&state, &headers, operation, || async move {
        let control = WorkflowControlService::new(action_state.db());
        let control_outcome = if pause {
            control.pause(&action_workflow_id).await
        } else {
            control.resume(&action_workflow_id).await
        }
        .map_err(workflow_core_error)?;
        let workflow = WorkflowQueryService::new(action_state.db())
            .get_workflow(&action_workflow_id)
            .await
            .map_err(workflow_core_error)?
            .ok_or_else(|| {
                CoreError::NotFound(format!("workflow {action_workflow_id} not found"))
            })?;
        Ok(json!({ "workflow": workflow, "control": control_outcome }))
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn get_workflow_context(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(workflow_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let context = WorkflowQueryService::new(state.db())
        .get_workflow_context(&workflow_id, state.pontia_home())
        .await
        .map_err(map_workflow_error)?
        .ok_or_else(|| ExternalApiError::not_found(format!("workflow {workflow_id} not found")))?;
    Ok(ok(json!({ "context": context })))
}

fn parse_limit(limit: Option<&str>) -> Result<u32, ExternalApiError> {
    let limit = match limit {
        None => 50,
        Some(value) => value.parse::<u32>().map_err(|_| {
            ExternalApiError::custom(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "limit must be an integer from 1 to 100",
            )
        })?,
    };
    if !(1..=100).contains(&limit) {
        return Err(ExternalApiError::custom(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "limit must be an integer from 1 to 100",
        ));
    }
    Ok(limit)
}

fn workflow_core_error(error: WorkflowError) -> CoreError {
    match error {
        WorkflowError::Pontia(error) => error,
        WorkflowError::WorkflowNotFound(workflow_id) => {
            CoreError::NotFound(format!("workflow {workflow_id} not found"))
        }
        other => CoreError::Domain(other.to_string()),
    }
}

fn map_workflow_error(error: WorkflowError) -> ExternalApiError {
    match error {
        WorkflowError::Pontia(error) => error.into(),
        WorkflowError::InvalidObservation(workflow_id) => ExternalApiError::custom(
            StatusCode::CONFLICT,
            "state_conflict",
            format!("workflow {workflow_id} cannot be observed because its definition is invalid"),
        ),
        other => ExternalApiError::custom(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            other.to_string(),
        ),
    }
}

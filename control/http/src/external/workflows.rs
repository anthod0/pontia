use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
};
use pontia_application::AppState;
use pontia_workflow::{Error as WorkflowError, WorkflowQueryService};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    authentication::authenticate,
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

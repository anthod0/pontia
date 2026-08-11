use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};
use pontia_application::{AppState, SessionCommandService};
use pontia_workflow::{
    InitialHandoff, RunWorkflowRequest, SubmitWorkflowNodeRequest, WorkflowNodeDefinition,
    WorkflowScheduler,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{authentication::authenticate_internal_token, response::ApiError};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRunRequest {
    workflow_id: String,
    title: String,
    cwd: String,
    #[serde(default)]
    handoffs: Vec<WorkflowRunHandoff>,
    nodes: Vec<WorkflowRunNode>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowRunHandoff {
    name: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkflowRunNode {
    #[serde(rename = "type")]
    node_type: String,
    title: String,
    instructions: String,
    #[serde(default)]
    inputs: Vec<String>,
    output: String,
    execution_profile_id: Option<String>,
    execution_profile_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowSubmissionRequest {
    session_id: String,
    runtime_instance_id: String,
    output: String,
    content: String,
}

pub async fn run_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<WorkflowRunRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    authenticate_internal_token(
        &state,
        &headers,
        "Internal Workflow API token is not configured",
    )?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let workflow_id = request.workflow_id.clone();
    let scheduler = WorkflowScheduler::new(
        state.db(),
        SessionCommandService::new(state.db()),
        state.agent_events(),
        pontia_config::pontia_home_dir(),
    );
    let outcome = scheduler
        .run(RunWorkflowRequest {
            workflow_id,
            title: request.title,
            cwd: request.cwd,
            handoffs: request
                .handoffs
                .into_iter()
                .map(|handoff| InitialHandoff {
                    name: handoff.name,
                    content: handoff.content,
                })
                .collect(),
            nodes: request
                .nodes
                .into_iter()
                .map(|node| WorkflowNodeDefinition {
                    node_type: node.node_type,
                    title: node.title,
                    instructions: node.instructions,
                    inputs: node.inputs,
                    output: node.output,
                    execution_profile_id: node.execution_profile_id,
                    execution_profile_version: node.execution_profile_version,
                })
                .collect(),
        })
        .await
        .map_err(ApiError::from_workflow)?;
    Ok(Json(json!({
        "data": {
            "workflow_id": outcome.workflow_id,
            "node_id": outcome.node_id,
            "session_id": outcome.session_id,
        }
    })))
}

pub async fn submit_workflow_output(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<WorkflowSubmissionRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    authenticate_internal_token(
        &state,
        &headers,
        "Internal Workflow API token is not configured",
    )?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let scheduler = WorkflowScheduler::new(
        state.db(),
        SessionCommandService::new(state.db()),
        state.agent_events(),
        pontia_config::pontia_home_dir(),
    );
    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: request.session_id,
            runtime_instance_id: request.runtime_instance_id,
            output: request.output,
            content: request.content,
        })
        .await
        .map_err(ApiError::from_workflow)?;
    Ok(Json(json!({ "data": { "submitted": true } })))
}

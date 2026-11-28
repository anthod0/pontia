use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use pontia_application::AppState;
use pontia_workflow::WorkflowState;
use pontia_workflow::{
    InitialHandoff, RunWorkflowRequest, SubmitWorkflowNodeRequest, WorkflowNodeDefinition,
    WorkflowScheduler,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::response::{ApiError, ApiResponse, ok};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowRunRequest {
    session_id: String,
    runtime_id: String,
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
    phase: String,
    title: String,
    instructions: String,
    #[serde(default)]
    inputs: Vec<String>,
    output: String,
    execution_profile_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowSubmissionRequest {
    session_id: String,
    runtime_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowTerminationRequest {
    session_id: String,
    runtime_id: String,
}

pub async fn run_workflow(
    State((state, workflows)): State<(AppState, WorkflowState)>,
    request: Result<Json<WorkflowRunRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let workflow_id = request.workflow_id.clone();
    let scheduler = WorkflowScheduler::new(
        workflows,
        state.control().clone(),
        state.pontia_home().to_path_buf(),
    );
    let outcome = scheduler
        .run(RunWorkflowRequest {
            workflow_id,
            session_id: request.session_id,
            runtime_id: request.runtime_id,
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
                    phase: node.phase,
                    title: node.title,
                    instructions: node.instructions,
                    inputs: node.inputs,
                    output: node.output,
                    execution_profile_id: node.execution_profile_id,
                })
                .collect(),
        })
        .await
        .map_err(map_command_error)?;
    Ok(ok(json!({
        "workflow_id": outcome.workflow_id,
        "node_id": outcome.node_id,
        "session_id": outcome.session_id,
    })))
}

pub async fn terminate_workflow_node(
    State((state, workflows)): State<(AppState, WorkflowState)>,
    request: Result<Json<WorkflowTerminationRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    WorkflowScheduler::new(
        workflows,
        state.control().clone(),
        state.pontia_home().to_path_buf(),
    )
    .terminate(&request.session_id, &request.runtime_id)
    .await
    .map_err(map_command_error)?;
    Ok(ok(json!({"reported":true})))
}

pub async fn submit_workflow_output(
    State((state, workflows)): State<(AppState, WorkflowState)>,
    request: Result<Json<WorkflowSubmissionRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let scheduler = WorkflowScheduler::new(
        workflows,
        state.control().clone(),
        state.pontia_home().to_path_buf(),
    );
    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: request.session_id,
            runtime_id: request.runtime_id,
        })
        .await
        .map_err(map_command_error)?;
    Ok(ok(json!({ "submitted": true })))
}

fn map_command_error(error: pontia_workflow::Error) -> ApiError {
    use axum::http::StatusCode;
    use pontia_core::Error as CoreError;
    use pontia_workflow::Error as WorkflowError;

    match error {
        WorkflowError::Pontia(CoreError::Domain(message)) => {
            ApiError::custom(StatusCode::CONFLICT, "state_conflict", message)
        }
        WorkflowError::Pontia(error) => error.into(),
        WorkflowError::WorkflowNotFound(workflow_id) => {
            ApiError::not_found(format!("workflow {workflow_id} not found"))
        }
        WorkflowError::NodeForSessionNotFound(session_id) => ApiError::not_found(format!(
            "session {session_id} is not bound to a workflow Agent Node"
        )),
        WorkflowError::InvalidDefinition(message) => ApiError::invalid_request(message),
        WorkflowError::UnsupportedNodeType(node_type) => {
            ApiError::invalid_request(format!("unsupported Workflow Node type: {node_type}"))
        }
        WorkflowError::InvalidWorkflowId(workflow_id) => {
            ApiError::invalid_request(format!("invalid Workflow ID: {workflow_id}"))
        }
        WorkflowError::InvalidHandoffFileName(message) => {
            ApiError::invalid_request(format!("invalid Handoff file name: {message}"))
        }
        WorkflowError::WorkflowNotRunning { .. }
        | WorkflowError::RuntimeControlUnavailable { .. }
        | WorkflowError::RuntimeMismatch { .. }
        | WorkflowError::AgentFileUnavailable { .. } => {
            ApiError::custom(StatusCode::CONFLICT, "state_conflict", error.to_string())
        }
        WorkflowError::RootNodeNotFound(_)
        | WorkflowError::InvalidObservation(_)
        | WorkflowError::Io(_)
        | WorkflowError::Json(_) => ApiError::custom(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            error.to_string(),
        ),
    }
}

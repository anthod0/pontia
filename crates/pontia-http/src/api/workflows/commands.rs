use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::HeaderMap,
};
use pontia_application::{AppState, SessionCommandService};
use pontia_workflow::{
    ApplyWorkflowPatch, BlockWorkflowPatch, InitialHandoff, RequestWorkflowPatch,
    RunWorkflowRequest, SubmitWorkflowNodeRequest, WorkflowNodeDefinition, WorkflowPatchService,
    WorkflowScheduler,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::super::{
    authentication::authenticate,
    response::{ApiError, ApiResponse, ok},
};

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
    phase: String,
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
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPatchRequest {
    session_id: String,
    runtime_instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPatchApplyRequest {
    session_id: String,
    runtime_instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPatchBlockRequest {
    session_id: String,
    runtime_instance_id: String,
}

pub async fn run_workflow(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<WorkflowRunRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let workflow_id = request.workflow_id.clone();
    let scheduler = WorkflowScheduler::new(
        state.db(),
        SessionCommandService::new(
            state.event_ingest_service(),
            state.pontia_home().to_path_buf(),
        )
        .with_client_control(state.client_control()),
        state.pontia_home().to_path_buf(),
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
                    phase: node.phase,
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
        .map_err(map_command_error)?;
    Ok(ok(json!({
        "workflow_id": outcome.workflow_id,
        "node_id": outcome.node_id,
        "session_id": outcome.session_id,
    })))
}

pub async fn request_workflow_patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<WorkflowPatchRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let outcome = WorkflowPatchService::new(state.db(), state.pontia_home().to_path_buf())
        .request_patch(RequestWorkflowPatch {
            session_id: request.session_id,
            runtime_instance_id: request.runtime_instance_id,
        })
        .await
        .map_err(map_command_error)?;
    Ok(ok(json!({
        "patch_id": outcome.patch_id,
        "state": "requested",
    })))
}

pub async fn apply_workflow_patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<WorkflowPatchApplyRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let outcome = WorkflowPatchService::new(state.db(), state.pontia_home().to_path_buf())
        .apply_patch(ApplyWorkflowPatch {
            session_id: request.session_id,
            runtime_instance_id: request.runtime_instance_id,
        })
        .await
        .map_err(map_command_error)?;
    Ok(ok(json!({
        "patch_id": outcome.patch_id,
        "workflow_id": outcome.workflow_id,
        "outcome": outcome.outcome,
        "revision": outcome.revision,
    })))
}

pub async fn block_workflow_patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<WorkflowPatchBlockRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let outcome = WorkflowPatchService::new(state.db(), state.pontia_home().to_path_buf())
        .block_patch(BlockWorkflowPatch {
            session_id: request.session_id,
            runtime_instance_id: request.runtime_instance_id,
        })
        .await
        .map_err(map_command_error)?;
    Ok(ok(json!({
        "patch_id": outcome.patch_id,
        "workflow_id": outcome.workflow_id,
        "state": "blocked",
    })))
}

pub async fn submit_workflow_output(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: Result<Json<WorkflowSubmissionRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    authenticate(&state, &headers)?;
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let scheduler = WorkflowScheduler::new(
        state.db(),
        SessionCommandService::new(
            state.event_ingest_service(),
            state.pontia_home().to_path_buf(),
        )
        .with_client_control(state.client_control()),
        state.pontia_home().to_path_buf(),
    );
    scheduler
        .submit(SubmitWorkflowNodeRequest {
            session_id: request.session_id,
            runtime_instance_id: request.runtime_instance_id,
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
        | WorkflowError::RuntimeMismatch { .. }
        | WorkflowError::AgentFileUnavailable { .. } => {
            ApiError::custom(StatusCode::CONFLICT, "state_conflict", error.to_string())
        }
        WorkflowError::RootNodeNotFound(_)
        | WorkflowError::InvalidObservation(_)
        | WorkflowError::MissingCreatedSessionId
        | WorkflowError::RuntimeControlUnavailable { .. }
        | WorkflowError::Io(_)
        | WorkflowError::Json(_)
        | WorkflowError::TomlSerialization(_) => ApiError::custom(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            error.to_string(),
        ),
    }
}

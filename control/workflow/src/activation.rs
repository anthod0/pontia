use std::{collections::BTreeMap, path::Path};

use pontia_application::{CreateSessionRequest, InitialTaskRequest};
use pontia_storage_sqlite::{
    models::workflows::{WorkflowNodeRow, WorkflowRow},
    repositories::workflows::SqliteWorkflowRepository,
};
use serde_json::json;

use crate::{Error, SessionCreator, validation::validate_handoff_file_name};

#[derive(Debug)]
pub(crate) struct ActivationFailure {
    pub(crate) error: Error,
    pub(crate) failure_message: String,
}

pub(crate) async fn activate_node<S: SessionCreator>(
    sessions: &S,
    repository: &SqliteWorkflowRepository,
    workflow: &WorkflowRow,
    node: &WorkflowNodeRow,
    handoff_dir: &Path,
) -> std::result::Result<String, ActivationFailure> {
    repository
        .claim_node_activation(
            &workflow.workflow_id,
            &node.node_id,
            &uuid::Uuid::now_v7().to_string(),
        )
        .await
        .map_err(|error| ActivationFailure {
            failure_message: format!(
                "failed to claim activation for Workflow Agent Node {}: {error}",
                node.node_id
            ),
            error: error.into(),
        })?;

    let result = activate_claimed_node(sessions, repository, workflow, node, handoff_dir).await;
    if result.is_err()
        && let Err(error) = repository
            .release_node_activation(&workflow.workflow_id, &node.node_id)
            .await
    {
        tracing::error!(
            workflow_id = %workflow.workflow_id,
            node_id = %node.node_id,
            %error,
            "failed to release Workflow Agent Node activation claim"
        );
    }
    result
}

async fn activate_claimed_node<S: SessionCreator>(
    sessions: &S,
    repository: &SqliteWorkflowRepository,
    workflow: &WorkflowRow,
    node: &WorkflowNodeRow,
    handoff_dir: &Path,
) -> std::result::Result<String, ActivationFailure> {
    let initial_task = render_initial_task(node, handoff_dir).await?;
    let workflow_file = handoff_dir.with_file_name("workflow.toml");
    let session_id = sessions
        .create_session(session_request(
            workflow,
            node,
            initial_task,
            &workflow_file,
        ))
        .await
        .map_err(|error| ActivationFailure {
            failure_message: format!(
                "Session creation failed for Workflow Agent Node {}: {error}",
                node.node_id
            ),
            error,
        })?;
    repository
        .finish_node_activation(&node.node_id, &session_id)
        .await
        .map_err(|error| ActivationFailure {
            failure_message: format!(
                "failed to bind Session {session_id} to Workflow Agent Node {}: {error}",
                node.node_id
            ),
            error: error.into(),
        })?;
    Ok(session_id)
}

fn session_request(
    workflow: &WorkflowRow,
    node: &WorkflowNodeRow,
    initial_task: String,
    workflow_file: &Path,
) -> CreateSessionRequest {
    let runtime_environment = BTreeMap::from([
        (
            "PONTIA_WORKFLOW_FILE".to_string(),
            workflow_file.display().to_string(),
        ),
        (
            "PONTIA_WORKFLOW_ID".to_string(),
            workflow.workflow_id.clone(),
        ),
    ]);
    CreateSessionRequest {
        client_type: "pi".to_string(),
        title: Some(node.title.clone()),
        workspace: Some(workflow.cwd.clone()),
        workspace_id: None,
        handle: None,
        role: None,
        description: None,
        execution_profile_id: node.execution_profile_id.clone(),
        execution_profile_version: node.execution_profile_version.clone(),
        metadata: json!({}),
        initial_task: Some(InitialTaskRequest {
            input: initial_task,
            metadata: json!({}),
        }),
        runtime_environment,
    }
}

async fn render_initial_task(
    node: &WorkflowNodeRow,
    handoff_dir: &Path,
) -> std::result::Result<String, ActivationFailure> {
    let inputs: Vec<String> =
        serde_json::from_str(&node.inputs).map_err(|error| ActivationFailure {
            failure_message: format!(
                "Workflow Agent Node {} has invalid declared Handoff inputs: {error}",
                node.node_id
            ),
            error: error.into(),
        })?;
    let mut rendered_inputs = String::new();
    for input in inputs {
        validate_handoff_file_name(&input).map_err(|error| ActivationFailure {
            failure_message: format!(
                "Workflow Agent Node {} declared invalid Handoff input {input}: {error}",
                node.node_id
            ),
            error,
        })?;
        let bytes = tokio::fs::read(handoff_dir.join(&input))
            .await
            .map_err(|error| ActivationFailure {
                failure_message: format!(
                    "failed to read declared Handoff input {input} for Workflow Agent Node {}: {error}",
                    node.node_id
                ),
                error: error.into(),
            })?;
        let content = String::from_utf8(bytes).map_err(|error| ActivationFailure {
            failure_message: format!(
                "declared Handoff input {input} for Workflow Agent Node {} is not valid UTF-8",
                node.node_id
            ),
            error: std::io::Error::new(std::io::ErrorKind::InvalidData, error).into(),
        })?;
        rendered_inputs.push_str(&format!("\n## Input file: {input}\n\n{content}\n"));
    }
    validate_handoff_file_name(&node.output).map_err(|error| ActivationFailure {
        failure_message: format!(
            "Workflow Agent Node {} declared invalid Handoff output {}: {error}",
            node.node_id, node.output
        ),
        error,
    })?;

    Ok(format!(
        "# Workflow Agent Node\n\n\
         ## Instructions\n\n{}\n\
         {}\n\
         ## Workflow definition\n\n\
         The durable Workflow definition is available at `$PONTIA_WORKFLOW_FILE`.\n\n\
         ## Handoff protocol\n\n\
         Expected output: {}\n\n\
         Complete the work, then create a source file in the Session cwd containing the full output. \
         Submit that file with:\n\n\
         ```bash\n\
         pontia workflow submit --input <source-path> --output {}\n\
         ```\n",
        node.instructions, rendered_inputs, node.output, node.output
    ))
}

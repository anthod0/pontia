use std::path::Path;

use serde::Serialize;

use super::{WorkflowDetailView, WorkflowQueryService};
use crate::{Error, Result, validation::validate_handoff_file_name};

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowContextView {
    pub workflow: WorkflowDetailView,
    pub current_node: Option<WorkflowNodeContextView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowNodeContextView {
    pub instructions: String,
    pub inputs: Vec<WorkflowInputView>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowInputView {
    pub name: String,
    pub content: Option<String>,
}

impl WorkflowQueryService {
    pub async fn get_workflow_context(
        &self,
        workflow_id: &str,
        pontia_home: &Path,
    ) -> Result<Option<WorkflowContextView>> {
        let Some(workflow) = self.get_workflow(workflow_id).await? else {
            return Ok(None);
        };
        let current_node = match workflow.current_node_id.as_deref() {
            Some(current_node_id) => {
                let node = self
                    .workflows
                    .get_node(current_node_id)
                    .await?
                    .filter(|node| node.workflow_id == workflow_id)
                    .ok_or_else(|| Error::InvalidObservation(workflow_id.to_string()))?;
                let input_names: Vec<String> = serde_json::from_str(&node.inputs)?;
                let handoff_dir = pontia_home
                    .join("workflows")
                    .join(workflow_id)
                    .join("handoff");
                let mut inputs = Vec::with_capacity(input_names.len());
                for name in input_names {
                    validate_handoff_file_name(&name)?;
                    let content = match tokio::fs::read(handoff_dir.join(&name)).await {
                        Ok(bytes) => String::from_utf8(bytes).ok(),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                        Err(error) => return Err(error.into()),
                    };
                    inputs.push(WorkflowInputView { name, content });
                }
                Some(WorkflowNodeContextView {
                    instructions: node.instructions,
                    inputs,
                    output: node.output,
                })
            }
            None => None,
        };
        Ok(Some(WorkflowContextView {
            workflow,
            current_node,
        }))
    }
}

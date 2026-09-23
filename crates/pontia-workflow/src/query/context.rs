use std::path::{Component, Path};

use serde::Serialize;

use super::{WorkflowActivePatchView, WorkflowDetailView, WorkflowQueryService};
use crate::{Error, Result, validation::validate_handoff_file_name};

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowContextView {
    pub workflow: WorkflowDetailView,
    pub definition_file: String,
    pub active_patch: Option<WorkflowActivePatchView>,
    pub current_node: Option<WorkflowNodeContextView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowNodeContextView {
    pub instructions: String,
    pub inputs: Vec<WorkflowInputView>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowDocumentView {
    pub workflow_id: String,
    pub document_ref: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowInputView {
    pub name: String,
    pub content: Option<String>,
}

impl WorkflowQueryService {
    pub async fn get_workflow_snapshot(
        &self,
        workflow_id: &str,
        pontia_home: &Path,
    ) -> Result<Option<WorkflowDetailView>> {
        let Some(mut workflow) = self.get_workflow(workflow_id).await? else {
            return Ok(None);
        };
        workflow.definition_file = Some(definition_file(pontia_home, workflow_id));
        workflow.active_patch = self.active_patch(workflow_id).await?;
        Ok(Some(workflow))
    }

    pub async fn get_workflow_context(
        &self,
        workflow_id: &str,
        pontia_home: &Path,
    ) -> Result<Option<WorkflowContextView>> {
        let Some(workflow) = self.get_workflow_snapshot(workflow_id, pontia_home).await? else {
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
        let definition_file = definition_file(pontia_home, workflow_id);
        let active_patch = workflow.active_patch.clone();
        Ok(Some(WorkflowContextView {
            workflow,
            definition_file,
            active_patch,
            current_node,
        }))
    }

    pub async fn read_workflow_document(
        &self,
        workflow_id: &str,
        document_ref: &str,
        pontia_home: &Path,
    ) -> Result<Option<WorkflowDocumentView>> {
        if self.workflows.get_workflow(workflow_id).await?.is_none() {
            return Ok(None);
        }
        let safe_ref = Path::new(document_ref);
        if document_ref.is_empty()
            || safe_ref.is_absolute()
            || safe_ref
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(pontia_core::Error::Domain(
                "invalid Workflow document reference".to_string(),
            )
            .into());
        }
        let patches = self.workflows.list_patches(workflow_id).await?;
        let authorized = document_ref == "workflow.toml"
            || patches.iter().any(|patch| {
                patch.request_document_ref == document_ref
                    || patch.decision_document_ref.as_deref() == Some(document_ref)
                    || patch.reason_document_ref.as_deref() == Some(document_ref)
                    || patch.blocked_draft_ref.as_deref() == Some(document_ref)
            });
        if !authorized {
            return Err(pontia_core::Error::NotFound(format!(
                "Workflow document {document_ref} not found"
            ))
            .into());
        }
        let workflow_dir = pontia_home.join("workflows").join(workflow_id);
        let canonical_dir = tokio::fs::canonicalize(&workflow_dir)
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    Error::Pontia(pontia_core::Error::NotFound(format!(
                        "Workflow document {document_ref} not found"
                    )))
                } else {
                    error.into()
                }
            })?;
        let path = workflow_dir.join(safe_ref);
        let canonical_path = tokio::fs::canonicalize(&path).await.map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Error::Pontia(pontia_core::Error::NotFound(format!(
                    "Workflow document {document_ref} not found"
                )))
            } else {
                error.into()
            }
        })?;
        if !canonical_path.starts_with(canonical_dir) {
            return Err(pontia_core::Error::NotFound(format!(
                "Workflow document {document_ref} not found"
            ))
            .into());
        }
        let content = tokio::fs::read_to_string(canonical_path)
            .await
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::InvalidData {
                    Error::Pontia(pontia_core::Error::Domain(format!(
                        "Workflow document {document_ref} is not UTF-8"
                    )))
                } else {
                    error.into()
                }
            })?;
        Ok(Some(WorkflowDocumentView {
            workflow_id: workflow_id.to_string(),
            document_ref: document_ref.to_string(),
            content,
        }))
    }
}

fn definition_file(pontia_home: &Path, workflow_id: &str) -> String {
    pontia_home
        .join("workflows")
        .join(workflow_id)
        .join("workflow.toml")
        .display()
        .to_string()
}

use std::path::{Component, Path, PathBuf};

use pontia_storage_sqlite::{
    models::workflows::{WorkflowNodeRow, WorkflowRow},
    repositories::workflows::{
        ApplyWorkflowNodeRecord, ApplyWorkflowPatchRecord, BlockWorkflowPatchRecord,
        RequestWorkflowPatchRecord, SqliteWorkflowRepository,
    },
};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    AcceptedWorkflowDefinition, AcceptedWorkflowNode, ApplyWorkflowPatch,
    ApplyWorkflowPatchOutcome, BlockWorkflowPatch, BlockWorkflowPatchOutcome, DefinitionChangePlan,
    Error, PlannedNodeParent, RequestWorkflowPatch, RequestWorkflowPatchOutcome, Result,
    WorkflowDefinitionHandoff, WorkflowNodeDefinition,
    definition::accepted_definition_from_snapshot, plan_workflow_definition_change,
    render_accepted_workflow_definition,
};

pub struct WorkflowPatchService {
    repository: SqliteWorkflowRepository,
    pontia_home: PathBuf,
}

impl WorkflowPatchService {
    pub fn new(pool: SqlitePool, pontia_home: PathBuf) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(pool),
            pontia_home,
        }
    }

    pub async fn request_patch(
        &self,
        request: RequestWorkflowPatch,
    ) -> Result<RequestWorkflowPatchOutcome> {
        validate_pontia_home_boundary(&self.pontia_home)?;
        let node = self
            .repository
            .get_node_by_session(&request.session_id)
            .await?
            .ok_or_else(|| Error::NodeForSessionNotFound(request.session_id.clone()))?;
        let patch_id = format!("patch_{}", Uuid::now_v7());
        let request_document_ref = format!("patches/{patch_id}/request.md");
        let patch_dir = self
            .pontia_home
            .join("workflows")
            .join(&node.workflow_id)
            .join("patches")
            .join(&patch_id);
        self.write_request_document(&patch_dir, &request.document)
            .await?;

        let accepted = self
            .repository
            .request_patch(RequestWorkflowPatchRecord {
                patch_id: patch_id.clone(),
                session_id: request.session_id,
                runtime_instance_id: request.runtime_instance_id,
                request_document_ref,
                request_size_bytes: i64::try_from(request.document.len()).map_err(|_| {
                    Error::InvalidDefinition("Workflow Patch request document is too large".into())
                })?,
                replanner_creation_token: format!("workflow_replanner_{}", Uuid::now_v7()),
                event_id: Uuid::now_v7().to_string(),
            })
            .await;
        match accepted {
            Ok(patch) => Ok(RequestWorkflowPatchOutcome {
                patch_id: patch.patch_id,
            }),
            Err(error) => {
                self.remove_unaccepted_document(&patch_dir).await;
                Err(error.into())
            }
        }
    }

    pub async fn apply_patch(
        &self,
        request: ApplyWorkflowPatch,
    ) -> Result<ApplyWorkflowPatchOutcome> {
        if request.decision.trim().is_empty() {
            return Err(Error::InvalidDefinition(
                "Workflow Patch decision must not be empty".into(),
            ));
        }
        validate_pontia_home_boundary(&self.pontia_home)?;
        let patch = self
            .repository
            .get_active_patch_for_replanner(&request.session_id, &request.runtime_instance_id)
            .await?
            .ok_or_else(|| {
                pontia_core::Error::StateConflict(format!(
                    "session {} is not the active Workflow Re-planner",
                    request.session_id
                ))
            })?;
        let workflow = self
            .repository
            .get_workflow(&patch.workflow_id)
            .await?
            .ok_or_else(|| Error::WorkflowNotFound(patch.workflow_id.clone()))?;
        if patch.base_revision != workflow.current_revision {
            return Err(pontia_core::Error::StateConflict(format!(
                "Workflow Patch {} base revision {} does not match current revision {}",
                patch.patch_id, patch.base_revision, workflow.current_revision
            ))
            .into());
        }

        let workflow_dir = self.pontia_home.join("workflows").join(&patch.workflow_id);
        let patch_dir = workflow_dir.join("patches").join(&patch.patch_id);
        self.validate_patch_directory(&workflow_dir, &patch_dir)?;
        let workflow_file = workflow_dir.join("workflow.toml");
        let accepted_file = patch_dir.join("accepted-definition.toml");
        for path in [&workflow_file, &accepted_file] {
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(Error::InvalidWorkflowId(path.display().to_string()));
            }
        }
        let candidate = tokio::fs::read(&workflow_file).await?;
        let accepted_bytes = tokio::fs::read(&accepted_file).await?;
        let active_nodes = self.repository.list_nodes(&patch.workflow_id).await?;
        let history = self
            .repository
            .list_node_history(&patch.workflow_id)
            .await?;
        let accepted =
            accepted_definition_from_snapshot(&workflow, active_nodes, &history, &accepted_bytes)?;
        let plan = plan_workflow_definition_change(&accepted, &candidate)?;

        let (retired_node_ids, introduced_nodes) = match plan {
            DefinitionChangePlan::NoChange => (Vec::new(), Vec::new()),
            DefinitionChangePlan::Changed {
                retired_node_ids,
                introduced_nodes,
                ..
            } => {
                let new_ids = introduced_nodes
                    .iter()
                    .map(|_| format!("node_{}", Uuid::now_v7()))
                    .collect::<Vec<_>>();
                let records = introduced_nodes
                    .into_iter()
                    .enumerate()
                    .map(|(index, node)| {
                        let parent_node_id = match node.parent {
                            None => None,
                            Some(PlannedNodeParent::Retained(id)) => Some(id),
                            Some(PlannedNodeParent::Introduced(parent)) => {
                                Some(new_ids[parent].clone())
                            }
                        };
                        Ok(ApplyWorkflowNodeRecord {
                            node_id: new_ids[index].clone(),
                            parent_node_id,
                            phase: node.definition.phase,
                            title: node.definition.title,
                            instructions: node.definition.instructions,
                            inputs: serde_json::to_string(&node.definition.inputs)?,
                            output: node.definition.output,
                            execution_profile_id: node.definition.execution_profile_id,
                            execution_profile_version: node.definition.execution_profile_version,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                (retired_node_ids, records)
            }
        };
        let decision_document_ref = format!("patches/{}/decision.md", patch.patch_id);
        self.write_atomic(&patch_dir, "decision.md", request.decision.as_bytes())
            .await?;
        let decision_size_bytes = i64::try_from(request.decision.len()).map_err(|_| {
            Error::InvalidDefinition("Workflow Patch decision document is too large".into())
        })?;
        let decision_summary = bounded_summary(&request.decision, 500);
        let resolved = self
            .repository
            .apply_patch(ApplyWorkflowPatchRecord {
                session_id: request.session_id,
                runtime_instance_id: request.runtime_instance_id,
                decision_document_ref,
                decision_size_bytes,
                decision_summary,
                retired_node_ids,
                introduced_nodes,
                continuation_message_id: format!("msg_{}", Uuid::now_v7()),
                event_id: Uuid::now_v7().to_string(),
            })
            .await?;

        if let Err(error) = self.rewrite_accepted_definition(&resolved, &accepted).await {
            tracing::warn!(patch_id = %resolved.patch_id, %error, "accepted Patch committed but normalized Workflow definition rewrite will require recovery");
        }
        Ok(ApplyWorkflowPatchOutcome {
            patch_id: resolved.patch_id,
            workflow_id: resolved.workflow_id,
            outcome: resolved.state,
            revision: resolved.result_revision.expect("resolved Patch revision"),
        })
    }

    async fn rewrite_accepted_definition(
        &self,
        patch: &pontia_storage_sqlite::models::workflows::WorkflowPatchRow,
        base: &AcceptedWorkflowDefinition,
    ) -> Result<()> {
        let workflow = self
            .repository
            .get_workflow(&patch.workflow_id)
            .await?
            .ok_or_else(|| Error::WorkflowNotFound(patch.workflow_id.clone()))?;
        let nodes = self.repository.list_nodes(&patch.workflow_id).await?;
        let definition = accepted_from_graph(&workflow, nodes, base.handoffs.clone())?;
        let rendered = render_accepted_workflow_definition(&definition)?;
        let workflow_dir = self.pontia_home.join("workflows").join(&patch.workflow_id);
        self.write_atomic(&workflow_dir, "workflow.toml", rendered.as_bytes())
            .await
    }

    pub async fn block_patch(
        &self,
        request: BlockWorkflowPatch,
    ) -> Result<BlockWorkflowPatchOutcome> {
        if request.reason.trim().is_empty() {
            return Err(Error::InvalidDefinition(
                "Workflow Patch block reason must not be empty".into(),
            ));
        }
        validate_pontia_home_boundary(&self.pontia_home)?;
        let patch = self
            .repository
            .get_active_patch_for_replanner(&request.session_id, &request.runtime_instance_id)
            .await?
            .ok_or_else(|| {
                pontia_core::Error::StateConflict(format!(
                    "session {} is not the active Workflow Re-planner",
                    request.session_id
                ))
            })?;
        let workflow_dir = self.pontia_home.join("workflows").join(&patch.workflow_id);
        let patch_dir = workflow_dir.join("patches").join(&patch.patch_id);
        self.validate_patch_directory(&workflow_dir, &patch_dir)?;

        self.write_atomic(&patch_dir, "reason.md", request.reason.as_bytes())
            .await?;
        let workflow_file = workflow_dir.join("workflow.toml");
        let accepted_file = patch_dir.join("accepted-definition.toml");
        for path in [&accepted_file, &workflow_file] {
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(Error::InvalidWorkflowId(path.display().to_string()));
            }
        }
        let accepted = tokio::fs::read(&accepted_file).await?;
        let draft = tokio::fs::read(&workflow_file).await?;
        let blocked_draft_ref = if draft != accepted {
            self.write_atomic(&patch_dir, "blocked-draft.toml", &draft)
                .await?;
            Some(format!("patches/{}/blocked-draft.toml", patch.patch_id))
        } else {
            None
        };
        let reason_document_ref = format!("patches/{}/reason.md", patch.patch_id);
        let blocked = self
            .repository
            .block_patch(BlockWorkflowPatchRecord {
                session_id: request.session_id,
                runtime_instance_id: request.runtime_instance_id,
                reason_document_ref,
                blocked_draft_ref,
                event_id: Uuid::now_v7().to_string(),
            })
            .await?;
        self.write_atomic(&workflow_dir, "workflow.toml", &accepted)
            .await?;
        Ok(BlockWorkflowPatchOutcome {
            patch_id: blocked.patch_id,
            workflow_id: blocked.workflow_id,
        })
    }

    async fn write_atomic(&self, directory: &Path, name: &str, content: &[u8]) -> Result<()> {
        let pending = directory.join(format!(".{name}.tmp"));
        tokio::fs::write(&pending, content).await?;
        if let Err(error) = tokio::fs::rename(&pending, directory.join(name)).await {
            let _ = tokio::fs::remove_file(&pending).await;
            return Err(error.into());
        }
        Ok(())
    }

    fn validate_patch_directory(&self, workflow_dir: &Path, patch_dir: &Path) -> Result<()> {
        for path in [
            self.pontia_home.join("workflows"),
            workflow_dir.to_path_buf(),
            patch_dir.to_path_buf(),
        ] {
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(Error::InvalidWorkflowId(path.display().to_string()));
            }
        }
        Ok(())
    }

    async fn write_request_document(&self, patch_dir: &Path, document: &str) -> Result<()> {
        let workflow_dir = patch_dir
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| Error::InvalidWorkflowId(patch_dir.display().to_string()))?;
        let workflows_dir = self.pontia_home.join("workflows");
        let workflow_file = workflow_dir.join("workflow.toml");
        for path in [
            workflows_dir.as_path(),
            workflow_dir,
            workflow_file.as_path(),
        ] {
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() {
                return Err(Error::InvalidWorkflowId(path.display().to_string()));
            }
        }
        let patches_dir = workflow_dir.join("patches");
        match std::fs::symlink_metadata(&patches_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::InvalidWorkflowId(patches_dir.display().to_string()));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                tokio::fs::create_dir(&patches_dir).await?;
            }
            Err(error) => return Err(error.into()),
        }
        tokio::fs::create_dir(patch_dir).await?;
        let pending = patch_dir.join(".request.md.tmp");
        if let Err(error) = tokio::fs::write(&pending, document).await {
            self.remove_unaccepted_document(patch_dir).await;
            return Err(error.into());
        }
        if let Err(error) = tokio::fs::rename(&pending, patch_dir.join("request.md")).await {
            self.remove_unaccepted_document(patch_dir).await;
            return Err(error.into());
        }
        Ok(())
    }

    async fn remove_unaccepted_document(&self, patch_dir: &Path) {
        let _ = tokio::fs::remove_file(patch_dir.join(".request.md.tmp")).await;
        let _ = tokio::fs::remove_file(patch_dir.join("request.md")).await;
        let _ = tokio::fs::remove_dir(patch_dir).await;
    }
}

fn bounded_summary(document: &str, max_chars: usize) -> String {
    let normalized = document.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = normalized.chars();
    let summary = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{summary}…")
    } else {
        summary
    }
}

fn accepted_from_graph(
    workflow: &WorkflowRow,
    mut rows: Vec<WorkflowNodeRow>,
    handoffs: Vec<WorkflowDefinitionHandoff>,
) -> Result<AcceptedWorkflowDefinition> {
    let mut nodes = Vec::with_capacity(rows.len());
    let mut parent: Option<String> = None;
    while !rows.is_empty() {
        let matches = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.parent_node_id == parent)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err(Error::InvalidDefinition(
                "accepted Workflow graph is not one linear chain".into(),
            ));
        }
        let row = rows.remove(matches[0]);
        parent = Some(row.node_id.clone());
        nodes.push(AcceptedWorkflowNode {
            node_id: row.node_id,
            parent_node_id: row.parent_node_id,
            definition: WorkflowNodeDefinition {
                node_type: row.node_type,
                phase: row.phase,
                title: row.title,
                instructions: row.instructions,
                inputs: serde_json::from_str(&row.inputs)?,
                output: row.output,
                execution_profile_id: row.execution_profile_id,
                execution_profile_version: row.execution_profile_version,
            },
            activated: row.session_id.is_some(),
        });
    }
    Ok(AcceptedWorkflowDefinition {
        workflow_id: workflow.workflow_id.clone(),
        revision: workflow.current_revision,
        title: workflow.title.clone(),
        cwd: workflow.cwd.clone(),
        handoffs,
        nodes,
        retired_node_ids: Vec::new(),
    })
}

fn validate_pontia_home_boundary(pontia_home: &Path) -> Result<()> {
    if pontia_home.as_os_str().is_empty()
        || !pontia_home.is_absolute()
        || pontia_home.parent().is_none()
        || pontia_home
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::InvalidWorkflowId(pontia_home.display().to_string()));
    }
    match std::fs::symlink_metadata(pontia_home) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(Error::InvalidWorkflowId(pontia_home.display().to_string()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

use std::path::Path;

use super::WorkflowCoordinator;
use crate::{
    AgentEventSubscriber, Error, GracefulExitRequester, Result, SessionCreator,
    TurnInterruptionRequester, definition::definition_handoffs, patch::accepted_from_graph,
    render_accepted_workflow_definition,
};

impl<S, X, I, B> WorkflowCoordinator<S, X, I, B>
where
    S: SessionCreator + Send + Sync + 'static,
    X: GracefulExitRequester + Send + Sync + 'static,
    I: TurnInterruptionRequester + Send + Sync + 'static,
    B: AgentEventSubscriber + Send + Sync + 'static,
{
    pub(super) async fn reconcile_definition_file(&self, workflow_id: &str) -> Result<()> {
        let Some(workflow) = self.repository.get_workflow(workflow_id).await? else {
            return Ok(());
        };
        if workflow.state == "replanning" {
            return Ok(());
        }
        let Some(patch) = self.repository.get_latest_patch(workflow_id).await? else {
            return Ok(());
        };
        let workflow_dir = self.pontia_home.join("workflows").join(workflow_id);
        let snapshot = workflow_dir
            .join("patches")
            .join(&patch.patch_id)
            .join("accepted-definition.toml");
        validate_regular_directory(&workflow_dir)?;
        validate_regular_file(&snapshot)?;
        let snapshot_bytes = tokio::fs::read(snapshot).await?;
        let handoffs = definition_handoffs(&snapshot_bytes)?;
        let nodes = self.repository.list_nodes(workflow_id).await?;
        let accepted = accepted_from_graph(&workflow, nodes, handoffs)?;
        let rendered = render_accepted_workflow_definition(&accepted)?;
        let workflow_file = workflow_dir.join("workflow.toml");
        match std::fs::symlink_metadata(&workflow_file) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(Error::InvalidWorkflowId(
                    workflow_file.display().to_string(),
                ));
            }
            Ok(_) => {
                if tokio::fs::read(&workflow_file).await? == rendered.as_bytes() {
                    return Ok(());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        write_atomic(&workflow_dir, "workflow.toml", rendered.as_bytes()).await
    }
}

pub(super) fn validate_regular_directory(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Error::InvalidWorkflowId(path.display().to_string()));
    }
    Ok(())
}

pub(super) fn validate_regular_file(path: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(Error::InvalidWorkflowId(path.display().to_string()));
    }
    Ok(())
}

pub(super) async fn write_atomic(directory: &Path, name: &str, content: &[u8]) -> Result<()> {
    let pending = directory.join(format!(".{name}.tmp"));
    tokio::fs::write(&pending, content).await?;
    if let Err(error) = tokio::fs::rename(&pending, directory.join(name)).await {
        let _ = tokio::fs::remove_file(&pending).await;
        return Err(error.into());
    }
    Ok(())
}

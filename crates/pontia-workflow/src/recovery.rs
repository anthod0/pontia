use crate::{Result, validation::validate_handoff_file_name};
use crate::{models::WorkflowRecoveryRow, repository::WorkflowState};
use pontia_agent_control::{
    AgentControl, InboxCommandService, InputPayload, SendMode, SessionCommandService,
};
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct WorkflowRecoveryService {
    repository: WorkflowState,
    sessions: SessionCommandService,
    inbox: std::sync::Arc<InboxCommandService>,
    home: PathBuf,
}

impl WorkflowRecoveryService {
    pub fn new(workflows: WorkflowState, app: &AgentControl) -> Self {
        Self {
            repository: workflows,
            sessions: app.session_commands(),
            inbox: app.inbox_commands(),
            home: app.pontia_home().to_path_buf(),
        }
    }

    pub async fn retry(
        &self,
        workflow_id: &str,
        failure_event_id: &str,
    ) -> Result<WorkflowRecoveryRow> {
        Ok(self
            .repository
            .request_recovery(
                workflow_id,
                failure_event_id,
                &format!("recovery_{}", uuid::Uuid::now_v7()),
            )
            .await?)
    }

    pub(crate) async fn reconcile(&self, workflow_id: &str) -> Result<bool> {
        let Some(row) = self
            .repository
            .list_recoveries(workflow_id)
            .await?
            .into_iter()
            .find(|r| matches!(r.state.as_str(), "requested" | "preparing" | "dispatching"))
        else {
            return Ok(false);
        };
        if row.state == "preparing" {
            return Ok(true);
        }
        if row.state == "requested" {
            if !self
                .repository
                .claim_recovery_preparation(&row.recovery_id)
                .await?
            {
                return Ok(true);
            }
            if let Err(error) = self.prepare(&row).await {
                self.repository
                    .finish_recovery(&row.recovery_id, Some(&error.to_string()))
                    .await?;
                return Ok(true);
            }
        }
        if !self
            .repository
            .recovery_runtime_available(&row.recovery_id)
            .await?
        {
            self.repository
                .finish_recovery(
                    &row.recovery_id,
                    Some("Recovered Session exited or changed runtime before input delivery"),
                )
                .await?;
            return Ok(true);
        } else {
            if let Err(error) = self
                .inbox
                .send(
                    &row.session_id,
                    InputPayload::from(self.recovery_prompt(&row).await?),
                    SendMode::Immediate,
                )
                .await
            {
                self.repository
                    .finish_recovery(
                        &row.recovery_id,
                        Some(&format!("Recovery input delivery failed: {error}")),
                    )
                    .await?;
                return Ok(true);
            }
        }
        self.repository
            .finish_recovery(&row.recovery_id, None)
            .await?;
        Ok(true)
    }

    async fn prepare(&self, row: &WorkflowRecoveryRow) -> Result<()> {
        let node = self
            .repository
            .get_node(&row.node_id)
            .await?
            .ok_or_else(|| pontia_core::Error::NotFound("Recovery node missing".into()))?;
        let directory = self.home.join("workflows").join(&row.workflow_id);
        for path in [
            &self.home,
            self.home.join("workflows").as_path(),
            &directory,
            &directory.join("handoff"),
        ] {
            require_directory(path).await?;
        }
        let inputs: Vec<String> = serde_json::from_str(&node.inputs)?;
        for input in inputs {
            validate_handoff_file_name(&input)?;
            require_file(&directory.join("handoff").join(input)).await?;
        }
        validate_handoff_file_name(&node.output)?;
        let archives = directory.join("recoveries");
        tokio::fs::create_dir_all(&archives).await?;
        require_directory(&archives).await?;
        let archive = archives.join(&row.recovery_id);
        tokio::fs::create_dir(&archive).await?;
        let output = directory.join("handoff").join(&node.output);
        match tokio::fs::symlink_metadata(&output).await {
            Ok(_) => {
                require_file(&output).await?;
                tokio::fs::rename(&output, archive.join(&node.output)).await?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        self.sessions.resume_for_input(&row.session_id).await?;
        self.repository
            .start_recovery_delivery(&row.recovery_id)
            .await?;
        Ok(())
    }

    async fn recovery_prompt(&self, row: &WorkflowRecoveryRow) -> Result<String> {
        let node = self
            .repository
            .get_node(&row.node_id)
            .await?
            .ok_or_else(|| pontia_core::Error::NotFound("Recovery node missing".into()))?;
        let directory = self.home.join("workflows").join(&row.workflow_id);
        let archive = directory.join("recoveries").join(&row.recovery_id);
        let output = directory.join("handoff").join(&node.output);
        Ok(format!(
            "Recover your interrupted Workflow Agent Node {} in the same conversation. Earlier submitted nodes remain complete. Inspect your existing work and any external side effects before continuing; do not blindly repeat completed operations. Your previous unsubmitted output, if present, was archived under `{}` and is not an accepted Submission. Recheck your assigned instructions and upstream inputs, then write the full output to `{}` and run `pontia workflow submit`. Stop after successful submission.\n\nNode instructions:\n{}",
            node.title,
            archive.display(),
            output.display(),
            node.instructions
        ))
    }
}

async fn require_directory(path: &Path) -> Result<()> {
    if !tokio::fs::symlink_metadata(path)
        .await?
        .file_type()
        .is_dir()
    {
        return Err(pontia_core::Error::Domain(format!(
            "Expected a regular Workflow directory: {}",
            path.display()
        ))
        .into());
    }
    Ok(())
}

async fn require_file(path: &Path) -> Result<()> {
    if !tokio::fs::symlink_metadata(path)
        .await?
        .file_type()
        .is_file()
    {
        return Err(pontia_core::Error::Domain(format!(
            "Expected a regular Handoff file: {}",
            path.display()
        ))
        .into());
    }
    Ok(())
}

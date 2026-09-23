use std::collections::BTreeMap;

use pontia_application::{CreateSessionRequest, InitialTaskRequest};
use pontia_storage_sqlite::models::{
    events::WorkflowTerminalEventRow, workflows::WorkflowPatchRow,
};
use serde_json::json;
use uuid::Uuid;

use super::{AgentTerminal, WorkflowCoordinator};
use crate::{
    AgentEventSubscriber, Error, GracefulExitRequester, Result, SessionCreator,
    TurnInterruptionRequester,
};

impl<S, X, I, B> WorkflowCoordinator<S, X, I, B>
where
    S: SessionCreator + Send + Sync + 'static,
    X: GracefulExitRequester + Send + Sync + 'static,
    I: TurnInterruptionRequester + Send + Sync + 'static,
    B: AgentEventSubscriber + Send + Sync + 'static,
{
    pub(super) async fn reconcile_patch_request(&self, workflow_id: &str) -> Result<()> {
        let Some(patch) = self.repository.get_active_patch(workflow_id).await? else {
            return Ok(());
        };
        if self
            .repository
            .patch_requester_interrupted(&patch.patch_id)
            .await?
        {
            if patch.state == "requested" {
                if let Err(error) = self.ensure_replanner(&patch).await {
                    if is_permanent_side_effect_failure(&error) {
                        self.implicitly_block_patch(
                            &patch,
                            None,
                            &format!("Re-planner Session could not be created: {error}"),
                        )
                        .await?;
                    } else {
                        return Err(error);
                    }
                }
            } else {
                self.reconcile_unresolved_replanner(&patch).await?;
            }
            return Ok(());
        }
        if let Some(event) = self.requester_terminal_before_interruption(&patch).await? {
            self.implicitly_block_patch(
                &patch,
                None,
                &format!(
                    "Requester {} made continuation impossible before interruption was confirmed",
                    event.event_type
                ),
            )
            .await?;
            return Ok(());
        }
        if !self
            .repository
            .mark_patch_interruption_attempted(&patch.patch_id)
            .await?
        {
            return Ok(());
        }
        match self
            .interruptions
            .request_turn_interruption(
                &patch.requesting_session_id,
                &patch.requesting_turn_id,
                &patch.requesting_runtime_instance_id,
            )
            .await
        {
            Ok(()) => {
                self.repository
                    .mark_patch_interruption_requested(&patch.patch_id)
                    .await?;
            }
            Err(error) if is_permanent_side_effect_failure(&error) => {
                self.implicitly_block_patch(
                    &patch,
                    None,
                    &format!("Requester interruption is permanently unavailable: {error}"),
                )
                .await?;
            }
            Err(error) => {
                tracing::warn!(
                    workflow_id,
                    patch_id = %patch.patch_id,
                    session_id = %patch.requesting_session_id,
                    turn_id = %patch.requesting_turn_id,
                    %error,
                    "failed to request Workflow Patch-owned Turn interruption; coordinator will retry"
                );
            }
        }
        Ok(())
    }

    async fn ensure_replanner(
        &self,
        patch: &pontia_storage_sqlite::models::workflows::WorkflowPatchRow,
    ) -> Result<()> {
        let workflow = self
            .repository
            .get_workflow(&patch.workflow_id)
            .await?
            .ok_or_else(|| crate::Error::WorkflowNotFound(patch.workflow_id.clone()))?;
        let workflow_dir = self.pontia_home.join("workflows").join(&patch.workflow_id);
        let workflow_file = workflow_dir.join("workflow.toml");
        let patch_dir = workflow_dir.join("patches").join(&patch.patch_id);
        for path in [&workflow_dir, &patch_dir, &workflow_file] {
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() {
                return Err(crate::Error::InvalidWorkflowId(path.display().to_string()));
            }
        }
        let accepted_snapshot = patch_dir.join("accepted-definition.toml");
        match std::fs::symlink_metadata(&accepted_snapshot) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(crate::Error::InvalidWorkflowId(
                    accepted_snapshot.display().to_string(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if !accepted_snapshot.exists() {
            let definition = tokio::fs::read(&workflow_file).await?;
            let pending = patch_dir.join(".accepted-definition.toml.tmp");
            tokio::fs::write(&pending, definition).await?;
            match tokio::fs::rename(&pending, &accepted_snapshot).await {
                Ok(()) => {}
                Err(error) if accepted_snapshot.exists() => {
                    let _ = tokio::fs::remove_file(&pending).await;
                    tracing::debug!(%error, "another reconciliation preserved the accepted Workflow definition");
                }
                Err(error) => return Err(error.into()),
            }
        }

        let metadata_key = "workflow_replanner_creation_token";
        let session_id = match self
            .sessions
            .find_session_by_creation_token(metadata_key, &patch.replanner_creation_token)
            .await?
        {
            Some(session_id) => session_id,
            None => {
                let request =
                    tokio::fs::read_to_string(workflow_dir.join(&patch.request_document_ref))
                        .await?;
                let definition = tokio::fs::read_to_string(&workflow_file).await?;
                let decision_file = patch_dir.join("decision.md");
                let reason_file = patch_dir.join("reason.md");
                let initial_task = format!(
                    "# Workflow Re-planner\n\n## Patch request\n\n{request}\n\n## Current Workflow definition\n\n{definition}\n\n## Instructions\n\nInspect the compact Workflow context with `pontia workflow show` and write the revised definition directly to:\n\n`{}`\n\nTo apply it, write the decision directly to:\n\n`{}`\n\nThen run:\n\n`pontia workflow patch apply`\n\nTo block the Patch instead, write the reason directly to:\n\n`{}`\n\nThen run:\n\n`pontia workflow patch block`\n",
                    workflow_file.display(),
                    decision_file.display(),
                    reason_file.display(),
                );
                self.sessions
                    .create_session(CreateSessionRequest {
                        client_type: "pi".into(),
                        title: Some(format!("Re-plan {}", workflow.title)),
                        workspace: Some(workflow.cwd.clone()),
                        workspace_id: None,
                        handle: None,
                        role: Some("workflow_replanner".into()),
                        description: Some(format!("Resolve Workflow Patch {}", patch.patch_id)),
                        execution_profile_id: None,
                        execution_profile_version: None,
                        metadata: json!({
                            "role": "workflow_replanner",
                            "workflow_id": patch.workflow_id,
                            "workflow_patch_id": patch.patch_id,
                            "workflow_replanner_creation_token": patch.replanner_creation_token,
                        }),
                        initial_task: Some(InitialTaskRequest {
                            input: initial_task,
                            metadata: json!({ "workflow_patch_id": patch.patch_id }),
                        }),
                        runtime_environment: BTreeMap::from([
                            ("PONTIA_WORKFLOW_ID".into(), patch.workflow_id.clone()),
                            ("PONTIA_WORKFLOW_PATCH_ID".into(), patch.patch_id.clone()),
                        ]),
                    })
                    .await?
            }
        };
        self.repository
            .bind_patch_replanner(&patch.patch_id, &session_id, &Uuid::now_v7().to_string())
            .await?;
        Ok(())
    }

    async fn requester_terminal_before_interruption(
        &self,
        patch: &WorkflowPatchRow,
    ) -> Result<Option<WorkflowTerminalEventRow>> {
        let Some(event) = self
            .persisted_events
            .latest_workflow_terminal_event(
                &patch.requesting_session_id,
                Some(&patch.requesting_runtime_instance_id),
                Some(&patch.requesting_turn_id),
            )
            .await?
        else {
            return Ok(None);
        };
        let terminal = AgentTerminal::from_event_type(&event.event_type);
        if matches!(
            terminal,
            Some(
                AgentTerminal::SessionExited
                    | AgentTerminal::TurnCompleted
                    | AgentTerminal::TurnFailed
            )
        ) {
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    async fn reconcile_unresolved_replanner(&self, patch: &WorkflowPatchRow) -> Result<()> {
        let (Some(session_id), Some(runtime_instance_id)) = (
            patch.replanner_session_id.as_deref(),
            patch.replanner_runtime_instance_id.as_deref(),
        ) else {
            return Ok(());
        };
        let Some(event) = self
            .persisted_events
            .latest_workflow_terminal_event(session_id, Some(runtime_instance_id), None)
            .await?
        else {
            return Ok(());
        };
        let Some(terminal) = AgentTerminal::from_event_type(&event.event_type) else {
            return Ok(());
        };
        let turn_id = if terminal == AgentTerminal::SessionExited {
            event.turn_id.clone()
        } else {
            let Some(turn_id) = event.turn_id.clone() else {
                return Ok(());
            };
            Some(turn_id)
        };
        self.implicitly_block_patch(
            patch,
            turn_id,
            &format!(
                "Re-planner reported {} before resolving the Workflow Patch",
                event.event_type
            ),
        )
        .await
    }
}

fn is_permanent_side_effect_failure(error: &Error) -> bool {
    matches!(
        error,
        Error::RuntimeControlUnavailable { .. }
            | Error::Pontia(
                pontia_core::Error::CapabilityUnavailable(_)
                    | pontia_core::Error::NotFound(_)
                    | pontia_core::Error::StateConflict(_)
            )
    )
}

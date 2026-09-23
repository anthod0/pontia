use pontia_application::SubmitInboxMessageRequest;
use pontia_storage_sqlite::{
    models::workflows::WorkflowPatchRow, repositories::workflows::ImplicitBlockWorkflowPatchRecord,
};
use serde_json::json;
use uuid::Uuid;

use super::{
    AgentTerminal, WorkflowCoordinator,
    definition_file::{validate_regular_directory, validate_regular_file, write_atomic},
};
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
    pub(super) async fn implicitly_block_patch(
        &self,
        patch: &WorkflowPatchRow,
        replanner_turn_id: Option<String>,
        reason: &str,
    ) -> Result<()> {
        let workflow_dir = self.pontia_home.join("workflows").join(&patch.workflow_id);
        let patch_dir = workflow_dir.join("patches").join(&patch.patch_id);
        validate_regular_directory(&workflow_dir)?;
        validate_regular_directory(&patch_dir)?;
        let token = Uuid::now_v7();
        let reason_name = format!("reason-{token}.md");
        write_atomic(&patch_dir, &reason_name, reason.as_bytes()).await?;
        let reason_document_ref = format!("patches/{}/{}", patch.patch_id, reason_name);

        let accepted_file = patch_dir.join("accepted-definition.toml");
        validate_regular_file(&accepted_file)?;
        let accepted = tokio::fs::read(&accepted_file).await?;
        let workflow_file = workflow_dir.join("workflow.toml");
        let draft = match std::fs::symlink_metadata(&workflow_file) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(Error::InvalidWorkflowId(
                    workflow_file.display().to_string(),
                ));
            }
            Ok(_) => Some(tokio::fs::read(&workflow_file).await?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let blocked_draft_ref = match draft {
            Some(draft) if draft != accepted => {
                let name = format!("blocked-draft-{token}.toml");
                write_atomic(&patch_dir, &name, &draft).await?;
                Some(format!("patches/{}/{}", patch.patch_id, name))
            }
            _ => None,
        };
        let blocked = self
            .repository
            .implicitly_block_patch(ImplicitBlockWorkflowPatchRecord {
                patch_id: patch.patch_id.clone(),
                replanner_turn_id,
                reason_document_ref,
                blocked_draft_ref,
                reason_summary: bounded_summary(reason, 500),
                event_id: Uuid::now_v7().to_string(),
            })
            .await?;
        if blocked {
            self.reconcile_definition_file(&patch.workflow_id).await?;
        }
        Ok(())
    }

    pub(super) async fn reconcile_patch_continuation(&self, workflow_id: &str) -> Result<bool> {
        let Some(patch) = self
            .repository
            .get_resolved_patch_awaiting_continuation(workflow_id)
            .await?
        else {
            return Ok(false);
        };
        let Some(message_id) = patch.continuation_message_id.as_deref() else {
            return Ok(false);
        };
        let Some(result_revision) = patch.result_revision else {
            return Ok(false);
        };
        let decision_ref = patch
            .decision_document_ref
            .as_deref()
            .unwrap_or("unavailable");
        let decision_path = self
            .pontia_home
            .join("workflows")
            .join(&patch.workflow_id)
            .join(decision_ref);
        let decision = match tokio::fs::read_to_string(&decision_path).await {
            Ok(document) => document,
            Err(error) => {
                tracing::warn!(patch_id = %patch.patch_id, %error, "cannot read Patch decision for continuation; coordinator will retry");
                return Ok(true);
            }
        };
        let input = format!(
            "Workflow Patch {} was {}. Continue Agent Node {} on accepted revision {}.\n\n## Re-planner decision\n\n{}",
            patch.patch_id, patch.state, patch.requesting_node_id, result_revision, decision,
        );
        self.inbox
            .submit_message_once(
                message_id,
                &patch.requesting_session_id,
                SubmitInboxMessageRequest {
                    input,
                    delivery_policy: "after_idle".into(),
                    branch_target_turn_id: None,
                    metadata: json!({
                        "workflow_patch_id": patch.patch_id,
                        "workflow_patch_outcome": patch.state,
                        "workflow_revision": result_revision,
                    }),
                },
            )
            .await?;
        self.repository
            .mark_patch_continuation_queued(&patch.patch_id, message_id)
            .await?;
        Ok(true)
    }

    pub(super) async fn reconcile_resolved_replanner(&self, workflow_id: &str) -> Result<()> {
        let Some(patch) = self
            .repository
            .get_resolved_patch_for_replanner(workflow_id)
            .await?
        else {
            return Ok(());
        };
        if patch.replanner_exit_requested_at.is_some() {
            return Ok(());
        }
        let (Some(session_id), Some(turn_id), Some(runtime_instance_id)) = (
            patch.replanner_session_id.as_deref(),
            patch.replanner_turn_id.as_deref(),
            patch.replanner_runtime_instance_id.as_deref(),
        ) else {
            return Ok(());
        };
        let Some(event) = self
            .persisted_events
            .latest_workflow_terminal_event(session_id, Some(runtime_instance_id), Some(turn_id))
            .await?
        else {
            return Ok(());
        };
        let terminal = AgentTerminal::from_event_type(&event.event_type);
        if !matches!(
            terminal,
            Some(
                AgentTerminal::TurnCompleted
                    | AgentTerminal::TurnFailed
                    | AgentTerminal::TurnInterrupted
            )
        ) || event.turn_id.as_deref() != Some(turn_id)
        {
            return Ok(());
        }
        if !self
            .repository
            .claim_patch_replanner_exit(&patch.patch_id)
            .await?
        {
            return Ok(());
        }
        if let Err(error) = self
            .exits
            .request_graceful_exit(session_id, runtime_instance_id)
            .await
        {
            self.repository
                .release_patch_replanner_exit(&patch.patch_id)
                .await?;
            tracing::warn!(patch_id = %patch.patch_id, session_id, %error, "failed to request graceful Re-planner exit; coordinator will retry");
        }
        Ok(())
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

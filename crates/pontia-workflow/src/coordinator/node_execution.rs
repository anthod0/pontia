use pontia_storage_sqlite::models::workflows::{WorkflowNodeRow, WorkflowRow};
use uuid::Uuid;

use super::{AgentTerminal, WorkflowCoordinator};
use crate::{
    AgentEventSubscriber, GracefulExitRequester, Result, SessionCreator, TurnInterruptionRequester,
    activation::activate_node,
};

impl<S, X, I, B> WorkflowCoordinator<S, X, I, B>
where
    S: SessionCreator + Send + Sync + 'static,
    X: GracefulExitRequester + Send + Sync + 'static,
    I: TurnInterruptionRequester + Send + Sync + 'static,
    B: AgentEventSubscriber + Send + Sync + 'static,
{
    async fn reconcile_unsubmitted(
        &self,
        workflow: &WorkflowRow,
        node: &WorkflowNodeRow,
        session_id: &str,
        terminal: AgentTerminal,
        runtime_instance_id: Option<String>,
        cause_event_id: &str,
    ) -> Result<()> {
        match terminal {
            AgentTerminal::TurnCompleted => {
                self.repository
                    .idle_unsubmitted_workflow_node(
                        &workflow.workflow_id,
                        &node.node_id,
                        &Uuid::now_v7().to_string(),
                        runtime_instance_id.as_deref(),
                    )
                    .await?;
            }
            AgentTerminal::TurnFailed
            | AgentTerminal::TurnInterrupted
            | AgentTerminal::SessionExited => {
                let failure_message = format!(
                    "Agent Client reported {} before Agent Node {} Submission",
                    terminal.event_type(),
                    node.node_id
                );
                self.repository
                    .fail_unsubmitted_workflow_node(
                        &workflow.workflow_id,
                        &node.node_id,
                        &Uuid::now_v7().to_string(),
                        &failure_message,
                        cause_event_id,
                        runtime_instance_id.as_deref(),
                    )
                    .await?;
                if terminal != AgentTerminal::SessionExited {
                    if let Some(runtime_instance_id) = runtime_instance_id {
                        if let Err(error) = self
                            .exits
                            .request_graceful_exit(session_id, &runtime_instance_id)
                            .await
                        {
                            tracing::warn!(workflow_id = %workflow.workflow_id, node_id = %node.node_id, session_id, %error, "failed to request graceful Session cleanup");
                        }
                    } else {
                        tracing::warn!(workflow_id = %workflow.workflow_id, node_id = %node.node_id, session_id, "cannot request graceful Session cleanup because the Agent fact has no runtime binding identity");
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) async fn reconcile_node_execution(
        &self,
        workflow_id: &str,
        workflow: &WorkflowRow,
    ) -> Result<()> {
        let nodes = self.repository.list_nodes(workflow_id).await?;
        let Some(node) = current_bound_node(&nodes) else {
            return Ok(());
        };
        let Some(session_id) = node.session_id.as_deref() else {
            return Ok(());
        };
        if self
            .repository
            .fail_for_turn_start_reporting(workflow_id, &node.node_id, &Uuid::now_v7().to_string())
            .await?
        {
            return Ok(());
        }
        let recovered_runtime = self
            .repository
            .recovered_node_runtime(&node.node_id)
            .await?;
        let Some(event) = self
            .persisted_events
            .latest_workflow_terminal_event(session_id, recovered_runtime.as_deref(), None)
            .await?
        else {
            return Ok(());
        };
        let Some(terminal) = AgentTerminal::from_event_type(&event.event_type) else {
            return Ok(());
        };
        let runtime_instance_id = event.runtime_instance_id.clone();
        if terminal == AgentTerminal::TurnInterrupted
            && (self
                .repository
                .terminal_event_precedes_latest_resume(workflow_id, &event.event_id)
                .await?
                || self
                    .repository
                    .terminal_event_is_resolved_patch_interruption(workflow_id, &event.event_id)
                    .await?)
        {
            return Ok(());
        }

        if node.submitted_at.is_none() {
            self.reconcile_unsubmitted(
                workflow,
                node,
                session_id,
                terminal,
                runtime_instance_id,
                &event.event_id,
            )
            .await?;
            return Ok(());
        }

        if terminal != AgentTerminal::SessionExited {
            let Some(submitted_runtime_instance_id) = node.submitted_runtime_instance_id.as_deref()
            else {
                tracing::error!(workflow_id, node_id = %node.node_id, "submitted Workflow Agent Node has no fenced runtime identity");
                return Ok(());
            };
            if !self
                .repository
                .claim_node_exit_request(&node.node_id, submitted_runtime_instance_id)
                .await?
            {
                return Ok(());
            }
            if let Err(error) = self
                .exits
                .request_graceful_exit(session_id, submitted_runtime_instance_id)
                .await
            {
                let failure_message = format!(
                    "graceful exit request failed for Workflow Session {session_id}: {error}"
                );
                self.repository
                    .fail_workflow(workflow_id, &Uuid::now_v7().to_string(), &failure_message)
                    .await?;
            }
            return Ok(());
        }

        let downstream = nodes
            .iter()
            .find(|candidate| candidate.parent_node_id.as_deref() == Some(&node.node_id));
        let Some(downstream) = downstream else {
            self.repository
                .complete_workflow(workflow_id, &Uuid::now_v7().to_string())
                .await?;
            return Ok(());
        };
        if downstream.session_id.is_some() {
            return Ok(());
        }

        let handoff_dir = self
            .pontia_home
            .join("workflows")
            .join(workflow_id)
            .join("handoff");
        if let Err(failure) = activate_node(
            &self.sessions,
            &self.repository,
            workflow,
            downstream,
            &handoff_dir,
        )
        .await
        {
            tracing::error!(workflow_id, node_id = %downstream.node_id, error = %failure.error, "failed to activate downstream Workflow Agent Node");
            self.repository
                .fail_workflow(
                    workflow_id,
                    &Uuid::now_v7().to_string(),
                    &failure.failure_message,
                )
                .await?;
        }
        Ok(())
    }
}

fn current_bound_node(nodes: &[WorkflowNodeRow]) -> Option<&WorkflowNodeRow> {
    nodes.iter().find(|node| {
        node.session_id.is_some()
            && !nodes.iter().any(|candidate| {
                candidate.parent_node_id.as_deref() == Some(&node.node_id)
                    && candidate.session_id.is_some()
            })
    })
}

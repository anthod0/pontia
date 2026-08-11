use std::{path::PathBuf, time::Duration};

use pontia_core::domain::{DomainEvent, EventSource, EventType};
use pontia_storage_sqlite::{
    models::workflows::WorkflowRow,
    repositories::{events::SqliteEventRepository, workflows::SqliteWorkflowRepository},
};
use uuid::Uuid;

use crate::{GracefulExitRequester, SessionCreator, activation::activate_node};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentTerminal {
    TurnCompleted,
    TurnFailed,
    TurnInterrupted,
    SessionExited,
}

impl AgentTerminal {
    fn from_event(event: &DomainEvent, session_id: &str) -> Option<Self> {
        if event.session_id != session_id || event.source != EventSource::AgentClient {
            return None;
        }
        Self::from_event_type(event.event_type)
    }

    fn from_event_type(event_type: EventType) -> Option<Self> {
        match event_type {
            EventType::TurnCompleted => Some(Self::TurnCompleted),
            EventType::TurnFailed => Some(Self::TurnFailed),
            EventType::TurnInterrupted => Some(Self::TurnInterrupted),
            EventType::SessionExited => Some(Self::SessionExited),
            _ => None,
        }
    }

    fn event_type(self) -> &'static str {
        match self {
            Self::TurnCompleted => "turn.completed",
            Self::TurnFailed => "turn.failed",
            Self::TurnInterrupted => "turn.interrupted",
            Self::SessionExited => "session.exited",
        }
    }
}

pub(crate) struct WorkflowMonitor<S, X> {
    pub(crate) repository: SqliteWorkflowRepository,
    pub(crate) persisted_events: SqliteEventRepository,
    pub(crate) sessions: S,
    pub(crate) exits: X,
    pub(crate) agent_events: tokio::sync::broadcast::Receiver<DomainEvent>,
    pub(crate) workflow: WorkflowRow,
    pub(crate) handoff_dir: PathBuf,
    pub(crate) node_id: String,
    pub(crate) session_id: String,
}

impl<S, X> WorkflowMonitor<S, X>
where
    S: SessionCreator + Clone + Send + Sync + 'static,
    X: GracefulExitRequester + Clone + Send + Sync + 'static,
{
    pub(crate) fn spawn(self) {
        tokio::spawn(self.run());
    }

    async fn run(mut self) {
        loop {
            let received = tokio::select! {
                received = self.agent_events.recv() => received,
                () = tokio::time::sleep(Duration::from_millis(250)) => {
                    match self.repository.get_workflow(&self.workflow.workflow_id).await {
                        Ok(Some(workflow)) if workflow.state == "running" => continue,
                        Ok(Some(_)) | Ok(None) => break,
                        Err(error) => {
                            tracing::error!(
                                workflow_id = %self.workflow.workflow_id,
                                %error,
                                "failed to reconcile Workflow state while waiting for Agent facts"
                            );
                            continue;
                        }
                    }
                }
            };
            let (terminal, runtime_instance_id) = match received {
                Ok(event) => match AgentTerminal::from_event(&event, &self.session_id) {
                    Some(terminal) => (
                        terminal,
                        event.payload["runtime_instance_id"]
                            .as_str()
                            .map(str::to_string),
                    ),
                    None => continue,
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    match self
                        .persisted_events
                        .latest_agent_client_terminal_event(&self.session_id)
                        .await
                    {
                        Ok(Some(event)) => {
                            let Some(terminal) = event
                                .event_type
                                .parse::<EventType>()
                                .ok()
                                .and_then(AgentTerminal::from_event_type)
                            else {
                                continue;
                            };
                            let runtime_instance_id =
                                serde_json::from_str::<serde_json::Value>(&event.payload)
                                    .ok()
                                    .and_then(|payload| {
                                        payload["runtime_instance_id"].as_str().map(str::to_string)
                                    });
                            tracing::warn!(
                                workflow_id = %self.workflow.workflow_id,
                                node_id = %self.node_id,
                                session_id = %self.session_id,
                                skipped,
                                event_type = %event.event_type,
                                "reconciled Workflow Agent Node from a durable terminal fact after lagged notifications"
                            );
                            (terminal, runtime_instance_id)
                        }
                        Ok(None) => continue,
                        Err(error) => {
                            tracing::error!(
                                workflow_id = %self.workflow.workflow_id,
                                node_id = %self.node_id,
                                session_id = %self.session_id,
                                skipped,
                                %error,
                                "failed to reconcile lagged workflow event notifications"
                            );
                            continue;
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            };
            match self
                .repository
                .get_workflow(&self.workflow.workflow_id)
                .await
            {
                Ok(Some(workflow)) if workflow.state == "running" => {}
                Ok(Some(_)) => break,
                Ok(None) => {
                    tracing::error!(
                        workflow_id = %self.workflow.workflow_id,
                        "workflow disappeared while handling an Agent terminal fact"
                    );
                    break;
                }
                Err(error) => {
                    tracing::error!(
                        workflow_id = %self.workflow.workflow_id,
                        %error,
                        "failed to load Workflow state after an Agent terminal fact"
                    );
                    continue;
                }
            }
            let submitted = match self.repository.get_node(&self.node_id).await {
                Ok(Some(node)) => node.submitted_at.is_some(),
                Ok(None) => {
                    tracing::error!(
                        workflow_id = %self.workflow.workflow_id,
                        node_id = %self.node_id,
                        "workflow node disappeared while handling confirmed Session exit"
                    );
                    break;
                }
                Err(error) => {
                    tracing::error!(
                        workflow_id = %self.workflow.workflow_id,
                        node_id = %self.node_id,
                        %error,
                        "failed to load workflow node after confirmed Session exit"
                    );
                    continue;
                }
            };
            if !submitted {
                match terminal {
                    AgentTerminal::TurnCompleted => {
                        if let Err(error) = self
                            .repository
                            .idle_unsubmitted_workflow_node(
                                &self.workflow.workflow_id,
                                &self.node_id,
                                &Uuid::now_v7().to_string(),
                            )
                            .await
                        {
                            tracing::error!(
                                workflow_id = %self.workflow.workflow_id,
                                node_id = %self.node_id,
                                %error,
                                "failed to idle Workflow after unsubmitted Turn completion"
                            );
                            continue;
                        }
                    }
                    AgentTerminal::TurnFailed
                    | AgentTerminal::TurnInterrupted
                    | AgentTerminal::SessionExited => {
                        let failure_message = format!(
                            "Agent Client reported {} before Agent Node {} Submission",
                            terminal.event_type(),
                            self.node_id
                        );
                        if let Err(error) = self
                            .repository
                            .fail_unsubmitted_workflow_node(
                                &self.workflow.workflow_id,
                                &self.node_id,
                                &Uuid::now_v7().to_string(),
                                &failure_message,
                            )
                            .await
                        {
                            tracing::error!(
                                workflow_id = %self.workflow.workflow_id,
                                node_id = %self.node_id,
                                %error,
                                "failed to persist Workflow failure after Agent terminal fact"
                            );
                            continue;
                        }
                        if terminal != AgentTerminal::SessionExited {
                            match runtime_instance_id {
                                Some(runtime_instance_id) => {
                                    if let Err(error) = self
                                        .exits
                                        .request_graceful_exit(
                                            &self.session_id,
                                            &runtime_instance_id,
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            workflow_id = %self.workflow.workflow_id,
                                            node_id = %self.node_id,
                                            session_id = %self.session_id,
                                            %error,
                                            "failed to request graceful Session cleanup"
                                        );
                                    }
                                }
                                None => tracing::warn!(
                                    workflow_id = %self.workflow.workflow_id,
                                    node_id = %self.node_id,
                                    session_id = %self.session_id,
                                    "cannot request graceful Session cleanup because the Agent fact has no runtime binding identity"
                                ),
                            }
                        }
                    }
                }
                break;
            }

            if terminal != AgentTerminal::SessionExited {
                continue;
            }

            let downstream = match self.repository.list_nodes(&self.workflow.workflow_id).await {
                Ok(nodes) => nodes
                    .into_iter()
                    .find(|node| node.parent_node_id.as_deref() == Some(&self.node_id)),
                Err(error) => {
                    tracing::error!(
                        workflow_id = %self.workflow.workflow_id,
                        node_id = %self.node_id,
                        %error,
                        "failed to find downstream workflow node after confirmed Session exit"
                    );
                    break;
                }
            };

            let Some(downstream) = downstream else {
                if let Err(error) = self
                    .repository
                    .complete_workflow(&self.workflow.workflow_id, &Uuid::now_v7().to_string())
                    .await
                {
                    tracing::error!(
                        workflow_id = %self.workflow.workflow_id,
                        node_id = %self.node_id,
                        %error,
                        "failed to complete workflow after confirmed Session exit"
                    );
                    continue;
                }
                break;
            };

            let downstream_session_id = match activate_node(
                &self.sessions,
                &self.repository,
                &self.workflow,
                &downstream,
                &self.handoff_dir,
            )
            .await
            {
                Ok(session_id) => session_id,
                Err(failure) => {
                    let transition_result = self
                        .repository
                        .fail_workflow(
                            &self.workflow.workflow_id,
                            &Uuid::now_v7().to_string(),
                            &failure.failure_message,
                        )
                        .await;
                    tracing::error!(
                        workflow_id = %self.workflow.workflow_id,
                        node_id = %downstream.node_id,
                        error = %failure.error,
                        "failed to activate downstream workflow Agent Node"
                    );
                    if let Err(error) = transition_result {
                        tracing::error!(
                            workflow_id = %self.workflow.workflow_id,
                            node_id = %downstream.node_id,
                            %error,
                            "failed to persist downstream Agent Node activation failure"
                        );
                    }
                    break;
                }
            };

            self.node_id = downstream.node_id;
            self.session_id = downstream_session_id;
        }
    }
}

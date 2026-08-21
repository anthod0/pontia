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
        if event.session_id != session_id {
            return None;
        }
        match (event.event_type, event.source) {
            (
                EventType::TurnCompleted | EventType::TurnFailed | EventType::TurnInterrupted,
                EventSource::AgentAdapter,
            ) => Self::from_event_type(event.event_type),
            (EventType::SessionExited, EventSource::AgentClient | EventSource::RuntimeManager) => {
                Some(Self::SessionExited)
            }
            _ => None,
        }
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
        let mut deferred_terminal = None;
        loop {
            let received = tokio::select! {
                received = self.agent_events.recv() => Some(received),
                () = tokio::time::sleep(Duration::from_millis(250)) => {
                    match self.repository.get_workflow(&self.workflow.workflow_id).await {
                        Ok(Some(workflow)) if workflow.state == "running" && deferred_terminal.is_some() => None,
                        Ok(Some(workflow)) if matches!(workflow.state.as_str(), "running" | "paused") => continue,
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
                None => deferred_terminal
                    .take()
                    .expect("deferred terminal was checked"),
                Some(Ok(event)) => match AgentTerminal::from_event(&event, &self.session_id) {
                    Some(terminal) => (
                        terminal,
                        event.payload["runtime_instance_id"]
                            .as_str()
                            .map(str::to_string),
                    ),
                    None => continue,
                },
                Some(Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped))) => {
                    match self
                        .persisted_events
                        .latest_workflow_terminal_event(&self.session_id)
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
                Some(Err(tokio::sync::broadcast::error::RecvError::Closed)) => break,
            };
            match self
                .repository
                .get_workflow(&self.workflow.workflow_id)
                .await
            {
                Ok(Some(workflow)) if workflow.state == "running" => {}
                Ok(Some(workflow)) if workflow.state == "paused" => {
                    if terminal != AgentTerminal::TurnInterrupted {
                        deferred_terminal = Some((terminal, runtime_instance_id));
                    }
                    continue;
                }
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
            let node = match self.repository.get_node(&self.node_id).await {
                Ok(Some(node)) => node,
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
            if node.submitted_at.is_none() {
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
                let Some(runtime_instance_id) = node.submitted_runtime_instance_id.as_deref()
                else {
                    tracing::error!(
                        workflow_id = %self.workflow.workflow_id,
                        node_id = %self.node_id,
                        "submitted Workflow Agent Node has no fenced runtime identity"
                    );
                    break;
                };
                match self
                    .repository
                    .claim_node_exit_request(&self.node_id, runtime_instance_id)
                    .await
                {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(error) => {
                        tracing::error!(
                            workflow_id = %self.workflow.workflow_id,
                            node_id = %self.node_id,
                            %error,
                            "failed to claim deferred graceful Session exit request"
                        );
                        continue;
                    }
                }
                if let Err(error) = self
                    .exits
                    .request_graceful_exit(&self.session_id, runtime_instance_id)
                    .await
                {
                    let failure_message = format!(
                        "graceful exit request failed for Workflow Session {}: {error}",
                        self.session_id
                    );
                    if let Err(transition_error) = self
                        .repository
                        .fail_workflow(
                            &self.workflow.workflow_id,
                            &Uuid::now_v7().to_string(),
                            &failure_message,
                        )
                        .await
                    {
                        tracing::error!(
                            workflow_id = %self.workflow.workflow_id,
                            node_id = %self.node_id,
                            %transition_error,
                            "failed to persist deferred graceful exit failure"
                        );
                    }
                    break;
                }
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

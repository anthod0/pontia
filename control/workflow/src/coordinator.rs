use std::{path::PathBuf, time::Duration};

use pontia_application::{AgentEventBroker, PiGracefulExitService};
use pontia_core::domain::EventType;
use pontia_storage_sqlite::{
    models::workflows::{WorkflowNodeRow, WorkflowRow},
    repositories::{events::SqliteEventRepository, workflows::SqliteWorkflowRepository},
};
use sqlx::SqlitePool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    AgentEventSubscriber, GracefulExitRequester, Result, SessionCreator, activation::activate_node,
};

const RECONCILIATION_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentTerminal {
    TurnCompleted,
    TurnFailed,
    TurnInterrupted,
    SessionExited,
}

impl AgentTerminal {
    fn from_event_type(event_type: &str) -> Option<Self> {
        match event_type.parse::<EventType>().ok()? {
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

/// Reconciles active Workflows from persisted Agent facts.
///
/// Realtime events only wake the coordinator. SQLite is reloaded before every
/// transition so startup recovery, missed notifications, and repeated passes
/// all follow the same path.
pub struct WorkflowCoordinator<S, X, B> {
    repository: SqliteWorkflowRepository,
    persisted_events: SqliteEventRepository,
    sessions: S,
    exits: X,
    agent_events: B,
    pontia_home: PathBuf,
}

impl<S> WorkflowCoordinator<S, PiGracefulExitService, AgentEventBroker>
where
    S: SessionCreator + Send + Sync + 'static,
{
    pub fn new(
        pool: SqlitePool,
        sessions: S,
        agent_events: AgentEventBroker,
        pontia_home: PathBuf,
    ) -> Self {
        let exits = PiGracefulExitService::new(pool.clone());
        Self::with_services(pool, sessions, exits, agent_events, pontia_home)
    }
}

impl<S, X, B> WorkflowCoordinator<S, X, B>
where
    S: SessionCreator + Send + Sync + 'static,
    X: GracefulExitRequester + Send + Sync + 'static,
    B: AgentEventSubscriber + Send + Sync + 'static,
{
    pub fn with_services(
        pool: SqlitePool,
        sessions: S,
        exits: X,
        agent_events: B,
        pontia_home: PathBuf,
    ) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(pool.clone()),
            persisted_events: SqliteEventRepository::new(pool),
            sessions,
            exits,
            agent_events,
            pontia_home,
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        if *shutdown.borrow() {
            return;
        }

        let mut agent_events = Some(self.agent_events.subscribe());
        let mut interval = tokio::time::interval(RECONCILIATION_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        self.reconcile_all().await;
        loop {
            tokio::select! {
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
                _ = interval.tick() => self.reconcile_all().await,
                received = async {
                    match agent_events.as_mut() {
                        Some(receiver) => Some(receiver.recv().await),
                        None => std::future::pending().await,
                    }
                } => {
                    match received.expect("active Agent event receiver") {
                        Ok(_) => self.reconcile_all().await,
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                            tracing::warn!(skipped, "Workflow Coordinator notification stream lagged; reconciling persisted facts");
                            self.reconcile_all().await;
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            tracing::warn!("Workflow Coordinator notification stream closed; continuing persisted reconciliation");
                            agent_events = None;
                            self.reconcile_all().await;
                        }
                    }
                }
            }
        }
    }

    async fn reconcile_all(&self) {
        let workflows = match self.repository.list_workflows_requiring_convergence().await {
            Ok(workflows) => workflows,
            Err(error) => {
                tracing::error!(%error, "failed to discover active Workflows for reconciliation");
                return;
            }
        };
        for workflow in workflows {
            if let Err(error) = self.reconcile(&workflow.workflow_id).await {
                tracing::error!(workflow_id = %workflow.workflow_id, %error, "failed to reconcile Workflow");
            }
        }
    }

    pub async fn reconcile(&self, workflow_id: &str) -> Result<()> {
        let Some(workflow) = self.repository.get_workflow(workflow_id).await? else {
            return Ok(());
        };
        if workflow.state != "running" {
            return Ok(());
        }
        let nodes = self.repository.list_nodes(workflow_id).await?;
        let Some(node) = current_bound_node(&nodes) else {
            return Ok(());
        };
        let Some(session_id) = node.session_id.as_deref() else {
            return Ok(());
        };
        let Some(event) = self
            .persisted_events
            .latest_workflow_terminal_event(session_id)
            .await?
        else {
            return Ok(());
        };
        let Some(terminal) = AgentTerminal::from_event_type(&event.event_type) else {
            return Ok(());
        };
        let runtime_instance_id = serde_json::from_str::<serde_json::Value>(&event.payload)
            .ok()
            .and_then(|payload| payload["runtime_instance_id"].as_str().map(str::to_string));
        if terminal == AgentTerminal::TurnInterrupted
            && self
                .repository
                .terminal_event_precedes_latest_resume(workflow_id, &event.event_id)
                .await?
        {
            return Ok(());
        }

        if node.submitted_at.is_none() {
            self.reconcile_unsubmitted(&workflow, node, session_id, terminal, runtime_instance_id)
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
            &workflow,
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

    async fn reconcile_unsubmitted(
        &self,
        workflow: &WorkflowRow,
        node: &WorkflowNodeRow,
        session_id: &str,
        terminal: AgentTerminal,
        runtime_instance_id: Option<String>,
    ) -> Result<()> {
        match terminal {
            AgentTerminal::TurnCompleted => {
                self.repository
                    .idle_unsubmitted_workflow_node(
                        &workflow.workflow_id,
                        &node.node_id,
                        &Uuid::now_v7().to_string(),
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

mod node_execution;

use std::{path::PathBuf, time::Duration};

use crate::repository::SqliteWorkflowRepository;
use pontia_agent_control::{
    AgentEventBroker, InboxCommandService, InputPayload, SendMode, SessionCommandService,
};
use pontia_core::domain::EventType;
use tokio::sync::watch;

use crate::{AgentEventSubscriber, GracefulExitRequester, Result, SessionCreator};

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
    sessions: S,
    exits: X,
    agent_events: B,
    inbox: std::sync::Arc<InboxCommandService>,
    pontia_home: PathBuf,
    recovery: crate::WorkflowRecoveryService,
}

impl<S> WorkflowCoordinator<S, SessionCommandService, AgentEventBroker>
where
    S: SessionCreator + Send + Sync + 'static,
{
    pub fn new(
        pool: sqlx::SqlitePool,
        app: &pontia_agent_control::AgentControl,
        sessions: S,
        agent_events: AgentEventBroker,
        pontia_home: PathBuf,
    ) -> Self {
        let exits = app.session_commands();
        Self::with_services(pool, app, sessions, exits, agent_events, pontia_home)
    }
}

impl<S, X, B> WorkflowCoordinator<S, X, B>
where
    S: SessionCreator + Send + Sync + 'static,
    X: GracefulExitRequester + Send + Sync + 'static,
    B: AgentEventSubscriber + Send + Sync + 'static,
{
    pub fn with_services(
        pool: sqlx::SqlitePool,
        app: &pontia_agent_control::AgentControl,
        sessions: S,
        exits: X,
        agent_events: B,
        pontia_home: PathBuf,
    ) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(pool.clone(), app.queries()),
            inbox: app.inbox_commands(),
            sessions,
            exits,
            agent_events,
            pontia_home,
            recovery: crate::WorkflowRecoveryService::new(pool, app),
        }
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        if *shutdown.borrow() {
            return;
        }

        let mut agent_events = Some(self.agent_events.subscribe());
        let mut interval = tokio::time::interval(RECONCILIATION_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        if let Err(error) = self.repository.recover_interrupted_preparations().await {
            tracing::error!(%error, "failed to reconcile interrupted Workflow recoveries");
        }
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
            if let Err(error) = self.report_events(&workflow).await {
                tracing::error!(workflow_id = %workflow.workflow_id, %error, "failed to queue Workflow report");
            }
        }
    }

    async fn report_events(&self, workflow: &crate::models::WorkflowRow) -> Result<()> {
        let id = &workflow.workflow_id;
        let handoff = self.pontia_home.join("workflows").join(id).join("handoff");
        for event in self.repository.pending_reports(id).await? {
            let payload: serde_json::Value = serde_json::from_str(&event.payload)?;
            let nodes = self.repository.list_nodes(id).await?;
            let mut input = format!("Workflow {} ({id}): {}", workflow.title, event.event_type);
            if let Some(node) = nodes
                .iter()
                .find(|node| Some(node.node_id.as_str()) == payload["node_id"].as_str())
            {
                input.push_str(&format!("\nNode: {} ({})", node.title, node.node_id));
                if let Some(session) = &node.session_id {
                    input.push_str(&format!("\nNode Session: {session}"));
                }
            }
            if let Some(reason) = payload["reason"]
                .as_str()
                .or_else(|| payload["failure_message"].as_str())
            {
                input.push_str(&format!("\n{reason}"));
            }
            input.push_str(&format!("\nHandoff directory: {}", handoff.display()));
            for node in nodes.iter().filter(|node| node.submitted_at.is_some()) {
                input.push_str(&format!(
                    "\nSubmitted output: {}",
                    handoff.join(&node.output).display()
                ));
            }
            input.push_str(&format!("\nInspect: pontia workflow show {id}"));
            self.inbox
                .send(
                    &workflow.initiator_session_id,
                    InputPayload::from(input),
                    SendMode::Queue,
                )
                .await
                .map_err(pontia_core::Error::from)?;
            self.repository
                .mark_report_queued(id, event.sequence)
                .await?;
        }
        Ok(())
    }

    pub async fn reconcile(&self, workflow_id: &str) -> Result<()> {
        self.repository.synchronize().await?;
        if self.recovery.reconcile(workflow_id).await? {
            return Ok(());
        }
        let Some(workflow) = self.repository.get_workflow(workflow_id).await? else {
            return Ok(());
        };
        if workflow.state != "running" {
            return Ok(());
        }
        self.reconcile_node_execution(workflow_id, &workflow).await
    }
}

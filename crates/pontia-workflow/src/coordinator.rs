mod definition_file;
mod node_execution;
mod patch_request;
mod patch_resolution;

use std::{path::PathBuf, time::Duration};

use pontia_application::{
    AgentEventBroker, InboxCommandService, SessionCommandService, TurnCommandService,
};
use pontia_core::domain::EventType;
use pontia_storage_sqlite::repositories::{
    events::SqliteEventRepository, workflows::SqliteWorkflowRepository,
};
use tokio::sync::watch;

use crate::{
    AgentEventSubscriber, GracefulExitRequester, Result, SessionCreator, TurnInterruptionRequester,
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
pub struct WorkflowCoordinator<S, X, I, B> {
    repository: SqliteWorkflowRepository,
    persisted_events: SqliteEventRepository,
    sessions: S,
    exits: X,
    interruptions: I,
    agent_events: B,
    inbox: InboxCommandService,
    pontia_home: PathBuf,
}

impl<S> WorkflowCoordinator<S, SessionCommandService, TurnCommandService, AgentEventBroker>
where
    S: SessionCreator + Send + Sync + 'static,
{
    pub fn new(
        event_ingest: pontia_application::EventIngestService,
        sessions: S,
        agent_events: AgentEventBroker,
        pontia_home: PathBuf,
    ) -> Self {
        let exits = SessionCommandService::new(event_ingest.clone(), pontia_home.clone());
        let interruptions = TurnCommandService::new(event_ingest.clone());
        Self::with_services_and_interruptions(
            event_ingest,
            sessions,
            exits,
            interruptions,
            agent_events,
            pontia_home,
        )
    }
}

impl<S, X, B> WorkflowCoordinator<S, X, X, B>
where
    S: SessionCreator + Send + Sync + 'static,
    X: GracefulExitRequester + TurnInterruptionRequester + Clone + Send + Sync + 'static,
    B: AgentEventSubscriber + Send + Sync + 'static,
{
    pub fn with_services(
        event_ingest: pontia_application::EventIngestService,
        sessions: S,
        exits: X,
        agent_events: B,
        pontia_home: PathBuf,
    ) -> Self {
        Self::with_services_and_interruptions(
            event_ingest,
            sessions,
            exits.clone(),
            exits,
            agent_events,
            pontia_home,
        )
    }
}

impl<S, X, I, B> WorkflowCoordinator<S, X, I, B>
where
    S: SessionCreator + Send + Sync + 'static,
    X: GracefulExitRequester + Send + Sync + 'static,
    I: TurnInterruptionRequester + Send + Sync + 'static,
    B: AgentEventSubscriber + Send + Sync + 'static,
{
    pub fn with_services_and_interruptions(
        event_ingest: pontia_application::EventIngestService,
        sessions: S,
        exits: X,
        interruptions: I,
        agent_events: B,
        pontia_home: PathBuf,
    ) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(event_ingest.db()),
            persisted_events: SqliteEventRepository::new(event_ingest.db()),
            inbox: InboxCommandService::new(event_ingest),
            sessions,
            exits,
            interruptions,
            agent_events,
            pontia_home,
        }
    }

    pub fn with_pi_control(mut self, control: pontia_application::PiControlService) -> Self {
        self.inbox = self.inbox.with_pi_control(control);
        self
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
        if workflow.state == "replanning" {
            return self.reconcile_patch_request(workflow_id).await;
        }
        if workflow.state == "blocked" {
            self.reconcile_resolved_replanner(workflow_id).await?;
            self.reconcile_definition_file(workflow_id).await?;
            return Ok(());
        }
        if workflow.state != "running" {
            return Ok(());
        }
        if self.reconcile_patch_continuation(workflow_id).await? {
            return Ok(());
        }
        self.reconcile_resolved_replanner(workflow_id).await?;
        self.reconcile_definition_file(workflow_id).await?;
        self.reconcile_node_execution(workflow_id, &workflow).await
    }
}

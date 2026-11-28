mod node_execution;

use std::{path::PathBuf, time::Duration};

use crate::repository::WorkflowState;
use pontia_agent_control::{InboxCommandService, InputPayload, SendMode, SessionCommandService};
use pontia_core::domain::EventType;
use tokio::sync::watch;

use crate::{GracefulExitRequester, Result, SessionCreator};

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

pub struct WorkflowRuntime<S, X> {
    repository: WorkflowState,
    sessions: S,
    exits: X,
    inbox: std::sync::Arc<InboxCommandService>,
    pontia_home: PathBuf,
    recovery: crate::WorkflowRecoveryService,
}

impl WorkflowRuntime<pontia_agent_control::AgentControl, SessionCommandService> {
    pub fn new(repository: WorkflowState, app: &pontia_agent_control::AgentControl) -> Self {
        Self::with_services(
            repository,
            app,
            app.clone(),
            app.session_commands(),
            app.pontia_home().into(),
        )
    }
}

impl<S, X> WorkflowRuntime<S, X>
where
    S: SessionCreator + Send + Sync + 'static,
    X: GracefulExitRequester + Send + Sync + 'static,
{
    pub fn with_services(
        repository: WorkflowState,
        app: &pontia_agent_control::AgentControl,
        sessions: S,
        exits: X,
        pontia_home: PathBuf,
    ) -> Self {
        Self {
            repository: repository.clone(),
            inbox: app.inbox_commands(),
            sessions,
            exits,
            pontia_home,
            recovery: crate::WorkflowRecoveryService::new(repository, app),
        }
    }

    pub async fn run(self, shutdown: watch::Receiver<bool>) -> pontia_core::Result<()> {
        let projection = self.repository.projection.clone();
        let (delivery, ()) = tokio::join!(
            projection.run(shutdown.clone()),
            self.run_reconciliation(shutdown)
        );
        delivery
    }

    async fn run_reconciliation(&self, mut shutdown: watch::Receiver<bool>) {
        if *shutdown.borrow() {
            return;
        }
        let mut recovering = true;
        loop {
            let result = async {
                if recovering {
                    self.repository.recover_interrupted_preparations().await?;
                    recovering = false;
                }
                self.reconcile_all().await
            }
            .await;
            let failed = result.is_err();
            if let Err(error) = result {
                tracing::error!(%error, "Workflow reconciliation failed");
            }
            tokio::select! {
                biased;
                _ = shutdown.wait_for(|stop| *stop) => break,
                _ = self.repository.changed.notified() => {},
                _ = tokio::time::sleep(Duration::from_secs(1)), if failed => {},
            }
        }
    }

    async fn reconcile_all(&self) -> Result<()> {
        let mut failure = None;
        for workflow in self
            .repository
            .list_workflows_requiring_convergence()
            .await?
        {
            for result in [
                self.reconcile(&workflow.workflow_id).await,
                self.report_events(&workflow).await,
            ] {
                if let Err(error) = result {
                    tracing::error!(workflow_id = %workflow.workflow_id, %error, "Workflow convergence failed");
                    failure = Some(error);
                }
            }
        }
        failure.map_or(Ok(()), Err)
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
        self.repository.projection.catch_up().await?;
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

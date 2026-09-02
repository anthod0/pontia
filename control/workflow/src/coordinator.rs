use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use pontia_application::{
    AgentEventBroker, CreateSessionRequest, InitialTaskRequest, PiGracefulExitService,
    RuntimeControlService,
};
use pontia_core::domain::EventType;
use pontia_storage_sqlite::{
    models::workflows::{WorkflowNodeRow, WorkflowRow},
    repositories::{events::SqliteEventRepository, workflows::SqliteWorkflowRepository},
};
use serde_json::json;
use sqlx::SqlitePool;
use tokio::sync::watch;
use uuid::Uuid;

use crate::{
    AgentEventSubscriber, GracefulExitRequester, Result, SessionCreator, TurnInterruptionRequester,
    activation::activate_node,
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
    pontia_home: PathBuf,
}

impl<S> WorkflowCoordinator<S, PiGracefulExitService, RuntimeControlService, AgentEventBroker>
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
        let interruptions = RuntimeControlService::new(pool.clone());
        Self::with_services_and_interruptions(
            pool,
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
        pool: SqlitePool,
        sessions: S,
        exits: X,
        agent_events: B,
        pontia_home: PathBuf,
    ) -> Self {
        Self::with_services_and_interruptions(
            pool,
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
        pool: SqlitePool,
        sessions: S,
        exits: X,
        interruptions: I,
        agent_events: B,
        pontia_home: PathBuf,
    ) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(pool.clone()),
            persisted_events: SqliteEventRepository::new(pool),
            sessions,
            exits,
            interruptions,
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
        if workflow.state == "replanning" {
            return self.reconcile_patch_request(workflow_id).await;
        }
        if workflow.state == "blocked" {
            return self.reconcile_blocked_replanner(workflow_id).await;
        }
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

    async fn reconcile_patch_request(&self, workflow_id: &str) -> Result<()> {
        let Some(patch) = self.repository.get_active_patch(workflow_id).await? else {
            return Ok(());
        };
        if self
            .repository
            .patch_requester_interrupted(&patch.patch_id)
            .await?
        {
            if patch.state == "requested" {
                self.ensure_replanner(&patch).await?;
            }
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
                let request_file = workflow_dir.join(&patch.request_document_ref);
                let initial_task = "# Workflow Re-planner\n\nInspect the compact Workflow context with `pontia workflow show`. \
                     Read the Patch request at `$PONTIA_WORKFLOW_PATCH_REQUEST_FILE`, edit \
                     `$PONTIA_WORKFLOW_FILE`, then resolve this Patch by invoking either \
                     `pontia workflow patch apply --decision <DECISION_FILE>` or \
                     `pontia workflow patch block --reason <REASON_FILE>`.\n"
                    .to_string();
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
                            (
                                "PONTIA_WORKFLOW_FILE".into(),
                                workflow_file.display().to_string(),
                            ),
                            ("PONTIA_WORKFLOW_PATCH_ID".into(), patch.patch_id.clone()),
                            (
                                "PONTIA_WORKFLOW_PATCH_REQUEST_FILE".into(),
                                request_file.display().to_string(),
                            ),
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

    async fn reconcile_blocked_replanner(&self, workflow_id: &str) -> Result<()> {
        let Some(patch) = self
            .repository
            .get_blocked_patch_for_replanner(workflow_id)
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
            .latest_workflow_terminal_event(session_id)
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
        let event_runtime = serde_json::from_str::<serde_json::Value>(&event.payload)
            .ok()
            .and_then(|payload| payload["runtime_instance_id"].as_str().map(str::to_string));
        if event_runtime.as_deref() != Some(runtime_instance_id)
            || !self
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

use std::path::{Component, Path, PathBuf};

use pontia_application::{AgentEventBroker, PiGracefulExitService};
use pontia_storage_sqlite::repositories::{
    events::SqliteEventRepository,
    workflows::{CreateWorkflowNodeRecord, CreateWorkflowRecord, SqliteWorkflowRepository},
};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    AgentEventSubscriber, Error, GracefulExitRequester, Result, RunWorkflowOutcome,
    RunWorkflowRequest, SessionCreator, StartWorkflowOutcome, SubmitWorkflowNodeRequest,
    activation::activate_node,
    monitor::WorkflowMonitor,
    validation::{
        is_runtime_control_unavailable, validate_handoff_file_name, validate_run_request,
    },
};

pub struct WorkflowScheduler<S, X, B> {
    repository: SqliteWorkflowRepository,
    persisted_events: SqliteEventRepository,
    sessions: S,
    exits: X,
    agent_events: B,
    pontia_home: PathBuf,
}

impl<S> WorkflowScheduler<S, PiGracefulExitService, AgentEventBroker>
where
    S: SessionCreator,
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

impl<S, X, B> WorkflowScheduler<S, X, B>
where
    S: SessionCreator,
    X: GracefulExitRequester,
    B: AgentEventSubscriber,
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

    pub async fn run(&self, request: RunWorkflowRequest) -> Result<RunWorkflowOutcome>
    where
        S: Clone + Send + Sync + 'static,
        X: Clone + Send + Sync + 'static,
    {
        validate_run_request(&request)?;
        validate_pontia_home_boundary(&self.pontia_home)?;

        let workflow_dir = self
            .pontia_home
            .join("workflows")
            .join(&request.workflow_id);
        let handoff_dir = workflow_dir.join("handoff");
        let workflows_dir = self.pontia_home.join("workflows");
        match std::fs::symlink_metadata(&workflows_dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(Error::InvalidWorkflowId(request.workflow_id));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        tokio::fs::create_dir_all(&workflows_dir).await?;
        tokio::fs::create_dir(&workflow_dir).await?;
        if let Err(error) = tokio::fs::create_dir(&handoff_dir).await {
            let _ = self.remove_workflow_tree(&workflow_dir).await;
            return Err(error.into());
        }
        for handoff in &request.handoffs {
            if let Err(error) =
                tokio::fs::write(handoff_dir.join(&handoff.name), &handoff.content).await
            {
                let _ = self.remove_workflow_tree(&workflow_dir).await;
                return Err(error.into());
            }
        }

        let mut parent_node_id = None;
        let mut nodes = Vec::with_capacity(request.nodes.len());
        for definition in request.nodes {
            let node_id = format!("node_{}", Uuid::now_v7());
            nodes.push(CreateWorkflowNodeRecord {
                node_id: node_id.clone(),
                workflow_id: request.workflow_id.clone(),
                parent_node_id: parent_node_id.clone(),
                phase: definition.phase.trim().to_string(),
                title: definition.title,
                instructions: definition.instructions,
                inputs: serde_json::to_string(&definition.inputs)?,
                output: definition.output,
                execution_profile_id: definition.execution_profile_id,
                execution_profile_version: definition.execution_profile_version,
            });
            parent_node_id = Some(node_id);
        }
        if let Err(error) = self
            .repository
            .create_definition(
                CreateWorkflowRecord {
                    workflow_id: request.workflow_id.clone(),
                    title: request.title,
                    cwd: request.cwd,
                    state: "pending".to_string(),
                },
                nodes,
            )
            .await
        {
            let _ = self.remove_workflow_tree(&workflow_dir).await;
            return Err(error.into());
        }

        let started = self.start(&request.workflow_id).await?;
        Ok(RunWorkflowOutcome {
            workflow_id: request.workflow_id,
            node_id: started.node_id,
            session_id: started.session_id,
        })
    }

    pub async fn start(&self, workflow_id: &str) -> Result<StartWorkflowOutcome>
    where
        S: Clone + Send + Sync + 'static,
        X: Clone + Send + Sync + 'static,
    {
        let agent_events = self.agent_events.subscribe();
        let workflow = self
            .repository
            .get_workflow(workflow_id)
            .await?
            .ok_or_else(|| Error::WorkflowNotFound(workflow_id.to_string()))?;
        let nodes = self.repository.list_nodes(workflow_id).await?;
        let root = nodes
            .into_iter()
            .find(|node| node.parent_node_id.is_none())
            .ok_or_else(|| Error::RootNodeNotFound(workflow_id.to_string()))?;

        self.repository
            .start_workflow(workflow_id, &Uuid::now_v7().to_string())
            .await?;
        validate_pontia_home_boundary(&self.pontia_home)?;
        let handoff_dir = self.handoff_dir(workflow_id);
        if let Err(error) = tokio::fs::create_dir_all(&handoff_dir).await {
            let failure_message = format!(
                "failed to create Workflow Handoff directory {}: {error}",
                handoff_dir.display()
            );
            self.repository
                .fail_workflow(workflow_id, &Uuid::now_v7().to_string(), &failure_message)
                .await?;
            return Err(error.into());
        }
        let session_id = match activate_node(
            &self.sessions,
            &self.repository,
            &workflow,
            &root,
            &handoff_dir,
        )
        .await
        {
            Ok(session_id) => session_id,
            Err(failure) => {
                self.repository
                    .fail_workflow(
                        workflow_id,
                        &Uuid::now_v7().to_string(),
                        &failure.failure_message,
                    )
                    .await?;
                return Err(failure.error);
            }
        };

        WorkflowMonitor {
            repository: self.repository.clone(),
            persisted_events: self.persisted_events.clone(),
            sessions: self.sessions.clone(),
            exits: self.exits.clone(),
            agent_events,
            workflow,
            handoff_dir,
            node_id: root.node_id.clone(),
            session_id: session_id.clone(),
        }
        .spawn();

        Ok(StartWorkflowOutcome {
            node_id: root.node_id,
            session_id,
        })
    }

    pub async fn submit(&self, request: SubmitWorkflowNodeRequest) -> Result<()> {
        let node = self
            .repository
            .get_node_by_session(&request.session_id)
            .await?
            .ok_or_else(|| Error::NodeForSessionNotFound(request.session_id.clone()))?;
        let workflow = self
            .repository
            .get_workflow(&node.workflow_id)
            .await?
            .ok_or_else(|| Error::WorkflowNotFound(node.workflow_id.clone()))?;
        if workflow.state != "running" {
            return Err(Error::WorkflowNotRunning {
                workflow_id: workflow.workflow_id,
                state: workflow.state,
            });
        }
        if let Err(error) = self
            .exits
            .ensure_current_runtime(&request.session_id, &request.runtime_instance_id)
            .await
        {
            if is_runtime_control_unavailable(&error) {
                let failure_message = format!(
                    "runtime binding is unavailable for Workflow Session {}: {error}",
                    request.session_id
                );
                self.repository
                    .fail_workflow(
                        &workflow.workflow_id,
                        &Uuid::now_v7().to_string(),
                        &failure_message,
                    )
                    .await?;
            }
            return Err(error);
        }
        if request.output != node.output {
            return Err(Error::OutputMismatch {
                expected: node.output,
                actual: request.output,
            });
        }
        validate_handoff_file_name(&request.output)?;
        validate_pontia_home_boundary(&self.pontia_home)?;
        tokio::fs::write(
            self.handoff_dir(&workflow.workflow_id)
                .join(&request.output),
            request.content,
        )
        .await?;
        self.repository
            .record_node_submission(&node.node_id)
            .await?;
        if let Err(error) = self
            .exits
            .request_graceful_exit(&request.session_id, &request.runtime_instance_id)
            .await
        {
            let failure_message = format!(
                "graceful exit request failed for Workflow Session {}: {error}",
                request.session_id
            );
            self.repository
                .fail_workflow(
                    &workflow.workflow_id,
                    &Uuid::now_v7().to_string(),
                    &failure_message,
                )
                .await?;
            return Err(error);
        }
        Ok(())
    }

    async fn remove_workflow_tree(&self, target: &Path) -> Result<()> {
        validate_pontia_home_boundary(&self.pontia_home)?;
        let workflows_dir = self.pontia_home.join("workflows");
        let name = target
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .ok_or_else(|| Error::InvalidWorkflowId(target.display().to_string()))?;
        if target.parent() != Some(workflows_dir.as_path())
            || target == workflows_dir
            || !name.starts_with("wf_")
        {
            return Err(Error::InvalidWorkflowId(target.display().to_string()));
        }
        for path in [&workflows_dir, target] {
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink() {
                return Err(Error::InvalidWorkflowId(target.display().to_string()));
            }
        }
        tokio::fs::remove_dir_all(target).await?;
        Ok(())
    }

    fn handoff_dir(&self, workflow_id: &str) -> PathBuf {
        self.pontia_home
            .join("workflows")
            .join(workflow_id)
            .join("handoff")
    }
}

fn validate_pontia_home_boundary(pontia_home: &Path) -> Result<()> {
    if pontia_home.as_os_str().is_empty()
        || !pontia_home.is_absolute()
        || pontia_home.parent().is_none()
        || pontia_home
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::InvalidWorkflowId(pontia_home.display().to_string()));
    }
    match std::fs::symlink_metadata(pontia_home) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(Error::InvalidWorkflowId(pontia_home.display().to_string()))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

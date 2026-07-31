//! Workflow orchestration over Pontia application services.

use std::{
    future::Future,
    path::{Component, Path, PathBuf},
};

use pontia_application::{
    AgentEventBroker, CreateSessionRequest, InitialTaskRequest, PiGracefulExitService,
    SessionCommandService,
};
use pontia_core::domain::{DomainEvent, EventSource, EventType};
use pontia_storage_sqlite::{
    models::workflows::{WorkflowNodeRow, WorkflowRow},
    repositories::{events::SqliteEventRepository, workflows::SqliteWorkflowRepository},
};
use serde_json::json;
use sqlx::SqlitePool;
use uuid::Uuid;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Pontia(#[from] pontia_core::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("workflow {0} not found")]
    WorkflowNotFound(String),

    #[error("workflow {0} has no root node")]
    RootNodeNotFound(String),

    #[error("session creation response did not contain a session_id")]
    MissingCreatedSessionId,

    #[error("invalid Handoff file name: {0}")]
    InvalidHandoffFileName(String),

    #[error("session {0} is not bound to a workflow Agent Node")]
    NodeForSessionNotFound(String),

    #[error("workflow {workflow_id} must be running, but is {state}")]
    WorkflowNotRunning { workflow_id: String, state: String },

    #[error("runtime {runtime_instance_id} is not the current runtime for session {session_id}")]
    RuntimeMismatch {
        session_id: String,
        runtime_instance_id: String,
    },

    #[error("output {actual} does not match Agent Node declared output {expected}")]
    OutputMismatch { expected: String, actual: String },
}

pub trait SessionCreator {
    fn create_session(
        &self,
        request: CreateSessionRequest,
    ) -> impl Future<Output = Result<String>> + Send;
}

impl SessionCreator for SessionCommandService {
    async fn create_session(&self, request: CreateSessionRequest) -> Result<String> {
        let outcome = SessionCommandService::create_session(self, request).await?;
        outcome
            .session_id()
            .map(str::to_string)
            .ok_or(Error::MissingCreatedSessionId)
    }
}

pub trait GracefulExitRequester {
    fn ensure_current_runtime(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

impl GracefulExitRequester for PiGracefulExitService {
    async fn ensure_current_runtime(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        PiGracefulExitService::ensure_current_runtime(self, session_id, runtime_instance_id)
            .await
            .map_err(Into::into)
    }

    async fn request_graceful_exit(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        self.request_exit(session_id, runtime_instance_id)
            .await
            .map_err(Into::into)
    }
}

pub trait AgentEventSubscriber {
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DomainEvent>;
}

impl AgentEventSubscriber for AgentEventBroker {
    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DomainEvent> {
        AgentEventBroker::subscribe(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitWorkflowNodeRequest {
    pub session_id: String,
    pub runtime_instance_id: String,
    pub output: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartWorkflowOutcome {
    pub node_id: String,
    pub session_id: String,
}

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

    pub async fn start(&self, workflow_id: &str) -> Result<StartWorkflowOutcome>
    where
        S: Clone + Send + Sync + 'static,
    {
        let mut agent_events = self.agent_events.subscribe();
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
        let handoff_dir = self.handoff_dir(workflow_id);
        tokio::fs::create_dir_all(&handoff_dir).await?;
        let session_id = activate_node(
            &self.sessions,
            &self.repository,
            &workflow,
            &root,
            &handoff_dir,
        )
        .await?;

        let repository = self.repository.clone();
        let persisted_events = self.persisted_events.clone();
        let sessions = self.sessions.clone();
        let handoff_dir = handoff_dir.clone();
        let watched_workflow_id = workflow.workflow_id.clone();
        let mut watched_node_id = root.node_id.clone();
        let mut watched_session_id = session_id.clone();
        tokio::spawn(async move {
            loop {
                let confirmed_exit = match agent_events.recv().await {
                    Ok(event) => {
                        event.session_id == watched_session_id
                            && event.source == EventSource::AgentClient
                            && event.client_type == "pi"
                            && event.event_type == EventType::SessionExited
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        match persisted_events
                            .has_agent_client_session_exit(&watched_session_id, "pi")
                            .await
                        {
                            Ok(confirmed_exit) => {
                                tracing::warn!(
                                    workflow_id = %watched_workflow_id,
                                    node_id = %watched_node_id,
                                    session_id = %watched_session_id,
                                    skipped,
                                    confirmed_exit,
                                    "reconciled workflow Agent Node after lagged event notifications"
                                );
                                confirmed_exit
                            }
                            Err(error) => {
                                tracing::error!(
                                    workflow_id = %watched_workflow_id,
                                    node_id = %watched_node_id,
                                    session_id = %watched_session_id,
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
                if !confirmed_exit {
                    continue;
                }
                let submitted = match repository.get_node(&watched_node_id).await {
                    Ok(Some(node)) => node.submitted_at.is_some(),
                    Ok(None) => {
                        tracing::error!(
                            workflow_id = %watched_workflow_id,
                            node_id = %watched_node_id,
                            "workflow node disappeared while handling confirmed Session exit"
                        );
                        break;
                    }
                    Err(error) => {
                        tracing::error!(
                            workflow_id = %watched_workflow_id,
                            node_id = %watched_node_id,
                            %error,
                            "failed to load workflow node after confirmed Session exit"
                        );
                        continue;
                    }
                };
                if !submitted {
                    break;
                }

                let downstream = match repository.list_nodes(&watched_workflow_id).await {
                    Ok(nodes) => nodes
                        .into_iter()
                        .find(|node| node.parent_node_id.as_deref() == Some(&watched_node_id)),
                    Err(error) => {
                        tracing::error!(
                            workflow_id = %watched_workflow_id,
                            node_id = %watched_node_id,
                            %error,
                            "failed to find downstream workflow node after confirmed Session exit"
                        );
                        break;
                    }
                };

                let Some(downstream) = downstream else {
                    if let Err(error) = repository
                        .complete_workflow(&watched_workflow_id, &Uuid::now_v7().to_string())
                        .await
                    {
                        tracing::error!(
                            workflow_id = %watched_workflow_id,
                            node_id = %watched_node_id,
                            %error,
                            "failed to complete workflow after confirmed Session exit"
                        );
                        continue;
                    }
                    break;
                };

                let downstream_session_id = match activate_node(
                    &sessions,
                    &repository,
                    &workflow,
                    &downstream,
                    &handoff_dir,
                )
                .await
                {
                    Ok(session_id) => session_id,
                    Err(error) => {
                        tracing::error!(
                            workflow_id = %watched_workflow_id,
                            node_id = %downstream.node_id,
                            %error,
                            "failed to activate downstream workflow Agent Node"
                        );
                        break;
                    }
                };

                watched_node_id = downstream.node_id;
                watched_session_id = downstream_session_id;
            }
        });

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
        self.exits
            .ensure_current_runtime(&request.session_id, &request.runtime_instance_id)
            .await?;
        if request.output != node.output {
            return Err(Error::OutputMismatch {
                expected: node.output,
                actual: request.output,
            });
        }
        validate_handoff_file_name(&request.output)?;
        tokio::fs::write(
            self.handoff_dir(&workflow.workflow_id)
                .join(&request.output),
            request.content,
        )
        .await?;
        self.repository
            .record_node_submission(&node.node_id)
            .await?;
        self.exits
            .request_graceful_exit(&request.session_id, &request.runtime_instance_id)
            .await
    }

    fn handoff_dir(&self, workflow_id: &str) -> PathBuf {
        self.pontia_home
            .join("workflows")
            .join(workflow_id)
            .join("handoff")
    }
}

async fn activate_node<S: SessionCreator>(
    sessions: &S,
    repository: &SqliteWorkflowRepository,
    workflow: &WorkflowRow,
    node: &WorkflowNodeRow,
    handoff_dir: &Path,
) -> Result<String> {
    let initial_task = render_initial_task(node, handoff_dir).await?;
    let session_id = sessions
        .create_session(session_request(workflow, node, initial_task))
        .await?;
    repository
        .bind_node_session(&node.node_id, &session_id)
        .await?;
    Ok(session_id)
}

fn session_request(
    workflow: &WorkflowRow,
    node: &WorkflowNodeRow,
    initial_task: String,
) -> CreateSessionRequest {
    CreateSessionRequest {
        client_type: "pi".to_string(),
        title: Some(node.title.clone()),
        workspace: Some(workflow.cwd.clone()),
        workspace_id: None,
        handle: None,
        role: None,
        description: None,
        execution_profile_id: node.execution_profile_id.clone(),
        execution_profile_version: node.execution_profile_version.clone(),
        metadata: json!({}),
        initial_task: Some(InitialTaskRequest {
            input: initial_task,
            metadata: json!({}),
        }),
    }
}

async fn render_initial_task(node: &WorkflowNodeRow, handoff_dir: &Path) -> Result<String> {
    let inputs: Vec<String> = serde_json::from_str(&node.inputs)?;
    let mut rendered_inputs = String::new();
    for input in inputs {
        validate_handoff_file_name(&input)?;
        let content = tokio::fs::read_to_string(handoff_dir.join(&input)).await?;
        rendered_inputs.push_str(&format!("\n## Input file: {input}\n\n{content}\n"));
    }
    validate_handoff_file_name(&node.output)?;

    Ok(format!(
        "# Workflow Agent Node\n\n\
         ## Instructions\n\n{}\n\
         {}\n\
         ## Handoff protocol\n\n\
         Expected output: {}\n\n\
         Complete the work, then create a source file in the Session cwd containing the full output. \
         Submit that file with:\n\n\
         ```bash\n\
         pontiactl workflow submit --input <source-path> --output {}\n\
         ```\n",
        node.instructions, rendered_inputs, node.output, node.output
    ))
}

fn validate_handoff_file_name(name: &str) -> Result<()> {
    let mut components = Path::new(name).components();
    if matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none() {
        return Ok(());
    }
    Err(Error::InvalidHandoffFileName(name.to_string()))
}

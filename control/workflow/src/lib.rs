//! Workflow orchestration over Pontia application services.

use std::{
    future::Future,
    path::{Component, Path, PathBuf},
};

use pontia_application::{CreateSessionRequest, InitialTaskRequest, SessionCommandService};
use pontia_storage_sqlite::{
    models::workflows::{WorkflowNodeRow, WorkflowRow},
    repositories::workflows::SqliteWorkflowRepository,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartWorkflowOutcome {
    pub node_id: String,
    pub session_id: String,
}

pub struct WorkflowScheduler<S> {
    repository: SqliteWorkflowRepository,
    sessions: S,
    pontia_home: PathBuf,
}

impl<S> WorkflowScheduler<S>
where
    S: SessionCreator,
{
    pub fn new(pool: SqlitePool, sessions: S, pontia_home: PathBuf) -> Self {
        Self {
            repository: SqliteWorkflowRepository::new(pool),
            sessions,
            pontia_home,
        }
    }

    pub async fn start(&self, workflow_id: &str) -> Result<StartWorkflowOutcome> {
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
        let initial_task = render_initial_task(&root, &handoff_dir).await?;
        let session_id = self
            .sessions
            .create_session(session_request(&workflow, &root, initial_task))
            .await?;
        self.repository
            .bind_node_session(&root.node_id, &session_id)
            .await?;

        Ok(StartWorkflowOutcome {
            node_id: root.node_id,
            session_id,
        })
    }

    fn handoff_dir(&self, workflow_id: &str) -> PathBuf {
        self.pontia_home
            .join("workflows")
            .join(workflow_id)
            .join("handoff")
    }
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

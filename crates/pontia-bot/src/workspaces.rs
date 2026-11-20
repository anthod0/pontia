use crate::identity::{BotIdentity, BotRole, IdentityError};
use pontia_agent_control::workspaces::{
    WorkspaceRecord, require_active_workspace, upsert_canonical_workspace,
};
use pontia_agent_control::{
    WorkspaceView,
    workspaces::{RegisterWorkspaceRequest, WorkspaceService},
};
use pontia_core::Error as ControlError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DescribeWorkspace {
    pub workspace_id: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "action",
    content = "input",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum WorkspaceCommand {
    Register(RegisterWorkspaceRequest),
    Describe(DescribeWorkspace),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceRequest {
    pub session_id: String,
    pub command: WorkspaceCommand,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Forbidden(#[from] IdentityError),
    #[error(transparent)]
    Control(#[from] pontia_core::Error),
}

#[derive(Clone)]
pub struct WorkspaceCommands {
    service: WorkspaceService,
    identity: Arc<dyn BotIdentity>,
}

impl WorkspaceCommands {
    pub fn new(service: WorkspaceService, identity: Arc<dyn BotIdentity>) -> Self {
        Self { service, identity }
    }

    pub async fn execute(&self, request: WorkspaceRequest) -> Result<WorkspaceView, Error> {
        let access = self.identity.resolve(&request.session_id).await?;
        if access.role != BotRole::Bot {
            return Err(IdentityError.into());
        }
        Ok(match request.command {
            WorkspaceCommand::Register(input) => self.service.register(input).await?,
            WorkspaceCommand::Describe(input) => {
                self.service
                    .update_description(&input.workspace_id, input.description.as_deref())
                    .await?
            }
        })
    }
}

pub(crate) async fn initialize_default(
    pool: &sqlx::SqlitePool,
    path: &Path,
) -> pontia_core::Result<WorkspaceRecord> {
    tokio::fs::create_dir_all(path).await?;
    let canonical = tokio::fs::canonicalize(path).await?;
    let path = canonical.to_string_lossy();
    match require_active_workspace(pool, &path).await {
        Err(ControlError::NotFound(_) | ControlError::StateConflict(_)) => {
            upsert_canonical_workspace(pool, &path, None).await
        }
        result => result,
    }
}

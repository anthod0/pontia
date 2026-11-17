use pontia_config::WorkspaceBrowserConfig;
use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::workspaces::SqliteWorkspaceRepository;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use super::{
    path_policy::{canonical_root, resolve_relative_path},
    upsert_canonical_workspace,
};
use crate::{ControlQueryService, WorkspaceView};

pub const WORKSPACE_DESCRIPTION_LIMIT: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RegisterWorkspaceRequest {
    pub root_id: String,
    #[serde(default)]
    pub path: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone)]
pub struct WorkspaceService {
    pool: SqlitePool,
    config: WorkspaceBrowserConfig,
}

impl WorkspaceService {
    pub fn new(pool: SqlitePool, config: WorkspaceBrowserConfig) -> Self {
        Self { pool, config }
    }

    pub async fn register(&self, request: RegisterWorkspaceRequest) -> Result<WorkspaceView> {
        validate_description(request.description.as_deref())?;
        let root = self
            .config
            .roots
            .iter()
            .find(|root| root.root_id == request.root_id)
            .ok_or_else(|| {
                Error::NotFound(format!("workspace root {} not found", request.root_id))
            })?;
        let root_path = canonical_root(root)?;
        let workspace_path = resolve_relative_path(&root_path, &request.path)?;
        if !workspace_path.is_dir() {
            return Err(Error::Domain(format!(
                "workspace path {:?} is not a directory",
                request.path
            )));
        }
        let record = upsert_canonical_workspace(
            &self.pool,
            &workspace_path.display().to_string(),
            request.name.as_deref(),
        )
        .await?;
        if let Some(description) = request.description {
            SqliteWorkspaceRepository::new(self.pool.clone())
                .update_description(&record.workspace_id, Some(&description))
                .await?;
        }
        self.get(&record.workspace_id).await
    }

    pub async fn update_description(
        &self,
        workspace_id: &str,
        description: Option<&str>,
    ) -> Result<WorkspaceView> {
        validate_description(description)?;
        let affected = SqliteWorkspaceRepository::new(self.pool.clone())
            .update_description(workspace_id, description)
            .await?;
        if affected == 0 {
            return Err(Error::NotFound(format!(
                "workspace {workspace_id} not found"
            )));
        }
        self.get(workspace_id).await
    }

    async fn get(&self, workspace_id: &str) -> Result<WorkspaceView> {
        ControlQueryService::new(self.pool.clone())
            .with_workspace_browser(self.config.clone())
            .get_workspace(workspace_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))
    }
}

fn validate_description(description: Option<&str>) -> Result<()> {
    if description.is_some_and(|text| text.chars().count() > WORKSPACE_DESCRIPTION_LIMIT) {
        return Err(Error::Domain(format!(
            "workspace description exceeds {WORKSPACE_DESCRIPTION_LIMIT} characters"
        )));
    }
    Ok(())
}

//! Read-only views of a Workspace's Git repository.

mod changes;
pub(crate) mod command;
mod diff;

use pontia_agent_control::get_workspace_record;
use pontia_core::error::{Error, Result};
use sqlx::SqlitePool;
use std::path::PathBuf;

pub use changes::{ChangeGroup, ChangedFileView, GitChangesView, MAX_CHANGED_FILES};
pub use diff::{DiffHunkView, DiffLineKind, DiffLineView, GitDiffView};

pub struct WorkspaceGitService {
    pool: SqlitePool,
}

impl WorkspaceGitService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn changes(&self, workspace_id: &str) -> Result<GitChangesView> {
        let Some(repo_root) = self.repo_root(workspace_id).await? else {
            return Ok(GitChangesView {
                repo_root: None,
                files: Vec::new(),
                total: 0,
                truncated: false,
            });
        };
        let (files, total) = changes::list_changes(&repo_root).await?;
        Ok(GitChangesView {
            repo_root: Some(repo_root.to_string_lossy().into_owned()),
            truncated: total > files.len(),
            files,
            total,
        })
    }

    pub async fn diff(
        &self,
        workspace_id: &str,
        path: &str,
        orig_path: Option<&str>,
        group: ChangeGroup,
    ) -> Result<GitDiffView> {
        let repo_root = self.repo_root(workspace_id).await?.ok_or_else(|| {
            Error::NotFound(format!(
                "workspace {workspace_id} is not in a Git repository"
            ))
        })?;
        diff::file_diff(&repo_root, path, orig_path, group).await
    }

    async fn repo_root(&self, workspace_id: &str) -> Result<Option<PathBuf>> {
        let workspace = get_workspace_record(&self.pool, workspace_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("workspace {workspace_id} not found")))?;
        let path = PathBuf::from(&workspace.canonical_path);
        if !path.is_dir() {
            return Err(Error::NotFound(format!(
                "workspace {workspace_id} directory is not available"
            )));
        }
        Ok(command::repo_root(&path).await?.map(PathBuf::from))
    }
}

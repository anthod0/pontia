//! Read-only views of a Workspace's Git repository.

mod changes;
pub(crate) mod command;
mod diff;
mod history;

use pontia_agent_control::get_workspace_record;
use pontia_core::error::{Error, Result};
use sqlx::SqlitePool;
use std::path::PathBuf;

pub use changes::{ChangeGroup, ChangedFileView, GitChangesView, MAX_CHANGED_FILES};
pub use diff::{DiffHunkView, DiffLineKind, DiffLineView, GitDiffView};
pub use history::{
    GitBranchView, GitBranchesView, GitCommitDiffView, GitCommitFileView, GitCommitSummaryView,
    GitCommitView, GitCommitsView,
};

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
        let repo_root = self.require_repo_root(workspace_id).await?;
        diff::file_diff(&repo_root, path, orig_path, group).await
    }

    pub async fn branches(&self, workspace_id: &str) -> Result<GitBranchesView> {
        let repo_root = self.require_repo_root(workspace_id).await?;
        let (head, branches) = history::branches(&repo_root).await?;
        Ok(GitBranchesView { head, branches })
    }

    /// One page of the history reachable from `ref_name`, newest first.
    pub async fn commits(
        &self,
        workspace_id: &str,
        ref_name: &str,
        skip: usize,
    ) -> Result<GitCommitsView> {
        let repo_root = self.require_repo_root(workspace_id).await?;
        history::commits(&repo_root, ref_name, skip).await
    }

    pub async fn commit(&self, workspace_id: &str, sha: &str) -> Result<GitCommitView> {
        let repo_root = self.require_repo_root(workspace_id).await?;
        history::commit(&repo_root, sha).await
    }

    pub async fn commit_diff(
        &self,
        workspace_id: &str,
        sha: &str,
        path: &str,
        orig_path: Option<&str>,
    ) -> Result<GitCommitDiffView> {
        let repo_root = self.require_repo_root(workspace_id).await?;
        history::commit_diff(&repo_root, sha, path, orig_path).await
    }

    async fn require_repo_root(&self, workspace_id: &str) -> Result<PathBuf> {
        self.repo_root(workspace_id).await?.ok_or_else(|| {
            Error::NotFound(format!(
                "workspace {workspace_id} is not in a Git repository"
            ))
        })
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

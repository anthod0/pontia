//! Read-only views of a Workspace's Git repository.

mod changes;
pub(crate) mod command;
mod diff;
mod history;

use pontia_config::WorkspaceBrowserConfig;
use pontia_core::error::{Error, Result};
use sqlx::SqlitePool;
use std::path::PathBuf;

use crate::GitRefreshCoordinator;

use self::command::GitCommand;

pub use changes::{ChangeGroup, ChangedFileView, GitChangesView, MAX_CHANGED_FILES};
pub use diff::{DiffHunkView, DiffLineKind, DiffLineView, GitDiffView};
pub use history::{
    GitBranchView, GitBranchesView, GitCommitDiffView, GitCommitFileView, GitCommitSummaryView,
    GitCommitView, GitCommitsView,
};

pub struct WorkspaceGitService {
    pool: SqlitePool,
    config: WorkspaceBrowserConfig,
    coordinator: GitRefreshCoordinator,
}

impl WorkspaceGitService {
    pub fn new(
        pool: SqlitePool,
        config: WorkspaceBrowserConfig,
        coordinator: GitRefreshCoordinator,
    ) -> Self {
        Self {
            pool,
            config,
            coordinator,
        }
    }

    pub async fn changes(&self, workspace_id: &str) -> Result<GitChangesView> {
        self.coordinator
            .scan(workspace_id, || async {
                self.changes_now(workspace_id).await
            })
            .await
    }

    pub async fn diff(
        &self,
        workspace_id: &str,
        path: &str,
        orig_path: Option<&str>,
        group: ChangeGroup,
    ) -> Result<GitDiffView> {
        let repository = self.require_repository(workspace_id).await?;
        diff::file_diff(&repository.command, path, orig_path, group).await
    }

    pub async fn branches(&self, workspace_id: &str) -> Result<GitBranchesView> {
        let repository = self.require_repository(workspace_id).await?;
        let (head, branches) = history::branches(&repository.command).await?;
        Ok(GitBranchesView { head, branches })
    }

    /// One page of the history reachable from `ref_name`, newest first.
    pub async fn commits(
        &self,
        workspace_id: &str,
        ref_name: &str,
        skip: usize,
        branch: Option<&str>,
    ) -> Result<GitCommitsView> {
        let repository = self.require_repository(workspace_id).await?;
        history::commits(&repository.command, ref_name, skip, branch).await
    }

    pub async fn commit(&self, workspace_id: &str, sha: &str) -> Result<GitCommitView> {
        let repository = self.require_repository(workspace_id).await?;
        history::commit(&repository.command, sha).await
    }

    pub async fn commit_diff(
        &self,
        workspace_id: &str,
        sha: &str,
        path: &str,
        orig_path: Option<&str>,
    ) -> Result<GitCommitDiffView> {
        let repository = self.require_repository(workspace_id).await?;
        history::commit_diff(&repository.command, sha, path, orig_path).await
    }

    async fn changes_now(&self, workspace_id: &str) -> Result<GitChangesView> {
        let Some(repository) = self.repository(workspace_id).await? else {
            return Ok(GitChangesView {
                repo_root: None,
                files: Vec::new(),
                total: 0,
                truncated: false,
            });
        };
        let (files, total) = changes::list_changes(&repository.command).await?;
        Ok(GitChangesView {
            repo_root: Some(repository.root.to_string_lossy().into_owned()),
            truncated: total > files.len(),
            files,
            total,
        })
    }

    async fn require_repository(&self, workspace_id: &str) -> Result<GitRepository> {
        self.repository(workspace_id).await?.ok_or_else(|| {
            Error::NotFound(format!(
                "workspace {workspace_id} is not in a Git repository"
            ))
        })
    }

    async fn repository(&self, workspace_id: &str) -> Result<Option<GitRepository>> {
        let path =
            crate::workspaces::accessible_workspace_path(&self.pool, &self.config, workspace_id)
                .await?;
        let command = GitCommand::new(&path).await?;
        Ok(command::repo_root(&command)
            .await?
            .map(|root| GitRepository {
                root: PathBuf::from(root),
                command,
            }))
    }
}

struct GitRepository {
    root: PathBuf,
    command: GitCommand,
}

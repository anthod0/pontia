use serde::Serialize;
use serde_json::Value;

use pontia_core::error::Result;
use pontia_storage_sqlite::models::{git_status::WorkspaceGitStatusRow, workspaces::WorkspaceRow};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkspaceView {
    pub workspace_id: String,
    pub canonical_path: String,
    pub display_path: String,
    pub name: Option<String>,
    pub state: String,
    pub metadata: Value,
    pub created_at: String,
    pub updated_at: String,
    pub last_used_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WorkspaceGitStatusView {
    pub workspace_id: String,
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub staged_count: i64,
    pub unstaged_count: i64,
    pub untracked_count: i64,
    pub conflicted_count: i64,
    pub clean: bool,
    pub state: String,
    pub failure: Option<String>,
    pub observed_at: Option<String>,
    pub updated_at: Option<String>,
}

pub(crate) fn row_to_view(row: WorkspaceRow) -> Result<WorkspaceView> {
    Ok(WorkspaceView {
        workspace_id: row.workspace_id,
        canonical_path: row.canonical_path,
        display_path: row.display_path,
        name: row.name,
        state: row.state,
        metadata: serde_json::from_str(&row.metadata)?,
        created_at: row.created_at,
        updated_at: row.updated_at,
        last_used_at: row.last_used_at,
    })
}

pub(crate) fn git_status_row_to_view(row: WorkspaceGitStatusRow) -> Result<WorkspaceGitStatusView> {
    Ok(WorkspaceGitStatusView {
        workspace_id: row.workspace_id,
        repo_root: row.repo_root,
        branch: row.branch,
        upstream: row.upstream,
        ahead: row.ahead,
        behind: row.behind,
        staged_count: row.staged_count,
        unstaged_count: row.unstaged_count,
        untracked_count: row.untracked_count,
        conflicted_count: row.conflicted_count,
        clean: row.clean,
        state: row.state,
        failure: row.failure,
        observed_at: row.observed_at,
        updated_at: row.updated_at,
    })
}

impl WorkspaceGitStatusView {
    pub fn unknown(workspace_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            repo_root: None,
            branch: None,
            upstream: None,
            ahead: 0,
            behind: 0,
            staged_count: 0,
            unstaged_count: 0,
            untracked_count: 0,
            conflicted_count: 0,
            clean: true,
            state: "unknown".to_string(),
            failure: None,
            observed_at: None,
            updated_at: None,
        }
    }
}

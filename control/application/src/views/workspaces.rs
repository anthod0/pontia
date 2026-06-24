use serde::Serialize;
use serde_json::Value;

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

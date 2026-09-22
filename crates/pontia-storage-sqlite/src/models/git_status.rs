#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceGitStatusRow {
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

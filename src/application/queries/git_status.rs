use super::*;

impl ExternalQueryService {
    pub async fn get_workspace_git_status(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceGitStatusView>> {
        let workspace_exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workspaces WHERE workspace_id = ? AND state != 'deleted'",
        )
        .bind(workspace_id)
        .fetch_one(&self.pool)
        .await?
            > 0;
        if !workspace_exists {
            return Ok(None);
        }

        let row = sqlx::query(
            r#"SELECT workspace_id, repo_root, branch, upstream, ahead, behind, staged_count,
                      unstaged_count, untracked_count, conflicted_count, clean, state, failure,
                      observed_at, updated_at
               FROM workspace_git_status
               WHERE workspace_id = ?"#,
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => row_to_workspace_git_status_view(row).map(Some),
            None => Ok(Some(WorkspaceGitStatusView::unknown(workspace_id))),
        }
    }
}

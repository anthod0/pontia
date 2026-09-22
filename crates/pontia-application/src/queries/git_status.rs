use pontia_core::error::Result;
use pontia_storage_sqlite::repositories::git_status::SqliteGitStatusRepository;

use super::ExternalQueryService;
use crate::views::workspaces::{WorkspaceGitStatusView, git_status_row_to_view};

impl ExternalQueryService {
    pub async fn get_workspace_git_status(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceGitStatusView>> {
        let repository = SqliteGitStatusRepository::new(self.pool.clone());
        let workspace_exists = repository.workspace_exists(workspace_id).await?;
        if !workspace_exists {
            return Ok(None);
        }

        let row = repository.get_status(workspace_id).await?;

        match row {
            Some(row) => git_status_row_to_view(row).map(Some),
            None => Ok(Some(WorkspaceGitStatusView::unknown(workspace_id))),
        }
    }
}

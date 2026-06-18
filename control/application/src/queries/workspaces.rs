use super::*;
use pontia_storage_sqlite::repositories::workspaces::SqliteWorkspaceRepository;

impl ExternalQueryService {
    pub async fn list_workspaces(&self) -> Result<Vec<WorkspaceView>> {
        let repository = SqliteWorkspaceRepository::new(self.pool.clone());
        let rows = repository.list_workspaces().await?;

        rows.into_iter().map(workspace_row_to_view).collect()
    }

    pub async fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceView>> {
        let repository = SqliteWorkspaceRepository::new(self.pool.clone());
        let row = repository.get_workspace(workspace_id).await?;

        row.map(workspace_row_to_view).transpose()
    }
}

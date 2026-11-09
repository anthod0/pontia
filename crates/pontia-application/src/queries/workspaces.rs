use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::workspaces::SqliteWorkspaceRepository;

use super::ExternalQueryService;
use crate::views::workspaces::{WorkspaceView, row_to_view};
use crate::workspaces::availability::WorkspaceAvailabilityProbe;

impl ExternalQueryService {
    pub async fn list_workspaces(&self) -> Result<Vec<WorkspaceView>> {
        let repository = SqliteWorkspaceRepository::new(self.pool.clone());
        let rows = repository.list_workspaces().await?;

        let config = self.workspace_browser.clone();
        tokio::task::spawn_blocking(move || {
            let probe = WorkspaceAvailabilityProbe::new(config);
            rows.into_iter()
                .map(|row| {
                    let availability = probe.check(&row.canonical_path);
                    row_to_view(row, availability)
                })
                .collect()
        })
        .await
        .map_err(|error| Error::Domain(format!("workspace availability task failed: {error}")))?
    }

    pub async fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceView>> {
        let repository = SqliteWorkspaceRepository::new(self.pool.clone());
        let row = repository.get_workspace(workspace_id).await?;

        let Some(row) = row else { return Ok(None) };
        let config = self.workspace_browser.clone();
        tokio::task::spawn_blocking(move || {
            let availability = WorkspaceAvailabilityProbe::new(config).check(&row.canonical_path);
            row_to_view(row, availability).map(Some)
        })
        .await
        .map_err(|error| Error::Domain(format!("workspace availability task failed: {error}")))?
    }
}

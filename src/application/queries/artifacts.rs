use super::*;
use pontia_storage_sqlite::repositories::artifacts::SqliteArtifactRepository;

impl ExternalQueryService {
    pub async fn list_artifacts(&self, session_id: &str) -> Result<Vec<ArtifactView>> {
        let repository = SqliteArtifactRepository::new(self.pool.clone());
        let rows = repository.list_artifacts(session_id).await?;

        rows.into_iter().map(artifact_row_to_view).collect()
    }

    pub async fn get_artifact(&self, artifact_id: &str) -> Result<Option<ArtifactView>> {
        let repository = SqliteArtifactRepository::new(self.pool.clone());
        let row = repository.get_artifact(artifact_id).await?;

        row.map(artifact_row_to_view).transpose()
    }
}

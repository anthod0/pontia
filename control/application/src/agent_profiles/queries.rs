use super::rows::row_to_execution_profile_view;
use super::*;
use pontia_storage_sqlite::repositories::agent_profiles::SqliteAgentProfileRepository;

impl AgentProfileService {
    pub async fn list_latest(&self) -> Result<Vec<ExecutionProfileView>> {
        let rows = SqliteAgentProfileRepository::new(self.pool.clone())
            .list_latest()
            .await?;

        rows.into_iter()
            .map(row_to_execution_profile_view)
            .collect()
    }

    pub async fn list_latest_including_archived(&self) -> Result<Vec<ExecutionProfileView>> {
        let rows = SqliteAgentProfileRepository::new(self.pool.clone())
            .list_latest_including_archived()
            .await?;

        rows.into_iter()
            .map(row_to_execution_profile_view)
            .collect()
    }

    pub async fn get_latest(&self, profile_id: &str) -> Result<Option<ExecutionProfileView>> {
        let row = SqliteAgentProfileRepository::new(self.pool.clone())
            .get_latest(profile_id)
            .await?;

        row.map(row_to_execution_profile_view).transpose()
    }

    pub async fn list_versions(
        &self,
        profile_id: &str,
        include_archived: bool,
    ) -> Result<Vec<ExecutionProfileView>> {
        let rows = SqliteAgentProfileRepository::new(self.pool.clone())
            .list_versions(profile_id, include_archived)
            .await?;

        rows.into_iter()
            .map(row_to_execution_profile_view)
            .collect()
    }

    pub async fn get_version(
        &self,
        profile_id: &str,
        version: &str,
    ) -> Result<Option<ExecutionProfileView>> {
        let row = SqliteAgentProfileRepository::new(self.pool.clone())
            .get_version(profile_id, version)
            .await?;

        row.map(row_to_execution_profile_view).transpose()
    }

    pub(super) async fn profile_exists(&self, profile_id: &str) -> Result<bool> {
        SqliteAgentProfileRepository::new(self.pool.clone())
            .profile_exists(profile_id)
            .await
    }
}

use super::{AgentProfileService, AgentProfileView};
use pontia_core::Result;
use pontia_storage_sqlite::repositories::agent_profiles::SqliteAgentProfileRepository;

impl AgentProfileService {
    pub async fn list(&self) -> Result<Vec<AgentProfileView>> {
        Ok(SqliteAgentProfileRepository::new(self.pool.clone())
            .list()
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub async fn get(&self, id: &str) -> Result<Option<AgentProfileView>> {
        Ok(SqliteAgentProfileRepository::new(self.pool.clone())
            .get(id)
            .await?
            .map(Into::into))
    }
}

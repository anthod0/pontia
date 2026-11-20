use super::{
    AgentProfileService, AgentProfileView, UpsertAgentProfileRequest,
    validation::{is_unique_constraint, validate_request},
};
use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::agent_profiles::{
    AgentProfileWriteRecord, SqliteAgentProfileRepository,
};

impl AgentProfileService {
    pub async fn initialize_profile(&self, request: UpsertAgentProfileRequest) -> Result<()> {
        if self.get(&request.profile_id).await?.is_some() {
            return Ok(());
        }
        self.validate_configuration(&request).await?;
        SqliteAgentProfileRepository::new(self.pool.clone())
            .initialize_profile(request.into())
            .await
    }

    async fn validate_configuration(&self, request: &UpsertAgentProfileRequest) -> Result<()> {
        validate_request(&self.clients, request)?;
        if let (Some(client), Some(key)) = (&request.client_type, &request.client_key) {
            self.validate_launch_client(client, key).await?;
        }
        Ok(())
    }

    pub async fn create(&self, request: UpsertAgentProfileRequest) -> Result<AgentProfileView> {
        self.validate_configuration(&request).await?;
        let id = request.profile_id.clone();
        if let Err(error) = SqliteAgentProfileRepository::new(self.pool.clone())
            .create(request.into())
            .await
        {
            if is_unique_constraint(&error) {
                return Err(Error::StateConflict(format!(
                    "agent profile {id} already exists"
                )));
            }
            return Err(error);
        }
        self.require_profile(&id).await
    }

    pub async fn update(
        &self,
        id: &str,
        request: UpsertAgentProfileRequest,
    ) -> Result<AgentProfileView> {
        if id != request.profile_id {
            return Err(Error::Domain(
                "profile_id in path must match request body".into(),
            ));
        }
        self.validate_configuration(&request).await?;
        if !SqliteAgentProfileRepository::new(self.pool.clone())
            .update(request.into())
            .await?
        {
            return Err(Error::NotFound(format!("agent profile {id} not found")));
        }
        self.require_profile(id).await
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        if !SqliteAgentProfileRepository::new(self.pool.clone())
            .delete(id)
            .await?
        {
            return Err(Error::NotFound(format!("agent profile {id} not found")));
        }
        Ok(())
    }

    pub(super) async fn require_profile(&self, id: &str) -> Result<AgentProfileView> {
        self.get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent profile {id} not found")))
    }
}

impl From<UpsertAgentProfileRequest> for AgentProfileWriteRecord {
    fn from(request: UpsertAgentProfileRequest) -> Self {
        Self {
            profile_id: request.profile_id,
            name: request.name,
            description: request.description,
            client_type: request.client_type,
            client_key: request.client_key,
            model: request.model,
            system_prompt: request.system_prompt,
        }
    }
}

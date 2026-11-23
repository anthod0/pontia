use super::{
    AgentProfileService, AgentProfileView, UpsertAgentProfileRequest,
    validation::{is_unique_constraint, validate_new_profile_id, validate_request},
};
use crate::ControlChange;
use pontia_core::{Error, Result};
use pontia_storage_sqlite::repositories::agent_profiles::{
    AgentProfileWriteRecord, SqliteAgentProfileRepository,
};

impl AgentProfileService {
    async fn validate_configuration(
        &self,
        request: &UpsertAgentProfileRequest,
        validate_model: bool,
    ) -> Result<()> {
        validate_request(&self.clients, request)?;
        if let (Some(client), Some(key)) = (&request.client_type, &request.client_key) {
            if validate_model && let Some(model) = &request.model {
                return crate::agent_clients::AgentClientService::new(
                    self.pool.clone(),
                    self.clients.clone(),
                )
                .validate_model_selection(
                    client,
                    key,
                    model,
                    request.reasoning_effort.as_deref(),
                    &self.home,
                )
                .await;
            }
            self.validate_launch_client(client, key).await?;
        }
        Ok(())
    }

    pub async fn create(
        &self,
        request: UpsertAgentProfileRequest,
        source_session_id: Option<&str>,
    ) -> Result<AgentProfileView> {
        validate_new_profile_id(&request.profile_id)?;
        self.validate_configuration(&request, request.reasoning_effort.is_some())
            .await?;
        let id = request.profile_id.clone();
        let name = request.name.clone();
        let description = request.description.clone();
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
        self.changes.publish(ControlChange::AgentProfileCreated {
            source_session_id: source_session_id.map(str::to_owned),
            profile_id: id.clone(),
            name,
            description,
        });
        self.require_profile(&id).await
    }

    pub async fn update(
        &self,
        id: &str,
        request: UpsertAgentProfileRequest,
        source_session_id: Option<&str>,
    ) -> Result<AgentProfileView> {
        let validate_model = request.reasoning_effort.is_some();
        self.update_with_model_check(id, request, source_session_id, validate_model)
            .await
    }

    /// Partial edits check the catalog only when the caller specifies a model or effort.
    pub async fn update_with_model_check(
        &self,
        id: &str,
        request: UpsertAgentProfileRequest,
        source_session_id: Option<&str>,
        validate_model: bool,
    ) -> Result<AgentProfileView> {
        if id != request.profile_id {
            return Err(Error::Domain(
                "profile_id in path must match request body".into(),
            ));
        }
        self.validate_configuration(&request, validate_model)
            .await?;
        let description = request.description.clone();
        let Some(changed) = SqliteAgentProfileRepository::new(self.pool.clone())
            .update(request.into())
            .await?
        else {
            return Err(Error::NotFound(format!("agent profile {id} not found")));
        };
        if changed {
            self.changes
                .publish(ControlChange::AgentProfileDescriptionChanged {
                    source_session_id: source_session_id.map(str::to_owned),
                    profile_id: id.into(),
                    description,
                });
        }
        self.require_profile(id).await
    }

    pub async fn delete(&self, id: &str, source_session_id: Option<&str>) -> Result<()> {
        if !SqliteAgentProfileRepository::new(self.pool.clone())
            .delete(id)
            .await?
        {
            return Err(Error::NotFound(format!("agent profile {id} not found")));
        }
        self.changes.publish(ControlChange::AgentProfileDeleted {
            source_session_id: source_session_id.map(str::to_owned),
            profile_id: id.into(),
        });
        Ok(())
    }

    pub async fn require_profile(&self, id: &str) -> Result<AgentProfileView> {
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
            reasoning_effort: request.reasoning_effort,
            system_prompt: request.system_prompt,
        }
    }
}

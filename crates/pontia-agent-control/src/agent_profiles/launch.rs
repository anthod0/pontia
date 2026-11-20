use super::AgentProfileService;
use pontia_core::{Error, Result};

impl AgentProfileService {
    pub async fn resolve_launch(
        &self,
        id: &str,
        version: Option<&str>,
    ) -> Result<super::ExecutionProfileView> {
        let profile = match version {
            Some(version) => self.get_version(id, version).await?,
            None => self.get_latest(id).await?,
        }
        .ok_or_else(|| Error::NotFound(format!("agent profile {id} not found")))?;
        if !profile.active {
            return Err(Error::Domain(format!("agent profile {id} is archived")));
        }
        Ok(profile)
    }

    pub async fn launch_defaults(
        &self,
        id: &str,
        version: Option<&str>,
    ) -> Result<super::ProfileLaunchDefaults> {
        let profile = self.resolve_launch(id, version).await?;
        let (Some(client_type), Some(client_key), Some(model)) = (
            profile.default_client_type,
            profile.default_client_key,
            profile.default_model,
        ) else {
            return Err(Error::Domain(format!(
                "agent profile {id} has no launch defaults"
            )));
        };
        if !profile.supported_client_types.contains(&client_type) {
            return Err(Error::Domain(format!(
                "agent profile {id} does not declare {client_type} support"
            )));
        }
        crate::agent_clients::AgentClientService::new(self.pool.clone(), self.clients.clone())
            .resolve(&client_type, &client_key)
            .await?;
        Ok(super::ProfileLaunchDefaults {
            profile_id: profile.profile_id,
            version: profile.version,
            client_type,
            client_key,
            model,
            system_prompt: profile.system_prompt_template,
        })
    }
}

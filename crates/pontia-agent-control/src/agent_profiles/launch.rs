use super::{AgentProfileService, ProfileLaunchConfig};
use pontia_core::{Error, Result};

impl AgentProfileService {
    pub(super) async fn validate_launch_client(&self, client: &str, key: &str) -> Result<()> {
        crate::agent_clients::AgentClientService::new(self.pool.clone(), self.clients.clone())
            .for_creation_key(client, key)
            .await?
            .ok_or_else(|| {
                Error::CapabilityUnavailable("Client configuration unavailable".into())
            })?;
        Ok(())
    }

    pub async fn resolve_launch(&self, id: &str) -> Result<ProfileLaunchConfig> {
        let profile = self.require_profile(id).await?;
        if let (Some(client), Some(key)) = (&profile.client_type, &profile.client_key) {
            self.validate_launch_client(client, key).await?;
        }
        Ok(ProfileLaunchConfig {
            client_type: profile.client_type,
            client_key: profile.client_key,
            model: profile.model,
            system_prompt: profile.system_prompt,
        })
    }
}

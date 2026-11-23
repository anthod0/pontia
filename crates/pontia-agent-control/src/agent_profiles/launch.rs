use super::{AgentProfileService, LaunchableAgentProfile, ProfileLaunchConfig};
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

    pub async fn launch_client_unavailable_reason(
        &self,
        client: &str,
        key: &str,
    ) -> Result<Option<String>> {
        match self.validate_launch_client(client, key).await {
            Ok(()) => Ok(None),
            Err(
                error @ (Error::CapabilityUnavailable(_)
                | Error::Domain(_)
                | Error::InvalidConfig { .. }
                | Error::Serialization(_)),
            ) => Ok(Some(error.to_string())),
            Err(error) => Err(error),
        }
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

    /// Profiles, ordered by ID, whose launch defaults currently pass Session creation
    /// admission. Profiles with an unavailable client are omitted; storage failures are returned.
    pub async fn list_launchable(&self) -> Result<Vec<LaunchableAgentProfile>> {
        let mut launchable = Vec::new();
        for profile in self.list().await? {
            let (Some(client_type), Some(client_key), Some(model)) =
                (profile.client_type, profile.client_key, profile.model)
            else {
                continue;
            };
            if self
                .launch_client_unavailable_reason(&client_type, &client_key)
                .await?
                .is_some()
            {
                continue;
            }
            launchable.push(LaunchableAgentProfile {
                profile_id: profile.profile_id,
                name: profile.name,
                description: profile.description,
                client_type,
                client_key,
                model,
            });
        }
        Ok(launchable)
    }
}

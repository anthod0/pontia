use super::{AgentProfileService, LaunchableAgentProfile, ProfileLaunchConfig};
use pontia_core::{Error, Result};

impl AgentProfileService {
    pub(super) fn validate_yolo(&self, client: &str, key: &str, use_yolo: bool) -> Result<()> {
        if use_yolo
            && !self
                .clients
                .get(client)
                .and_then(|entry| entry.configuration.as_ref())
                .is_some_and(|config| config.supports_mode(key, pontia_core::SessionMode::Yolo))
        {
            return Err(Error::Unsupported(format!(
                "Client {client}/{key} does not support YOLO mode"
            )));
        }
        Ok(())
    }

    pub(super) async fn validate_launch_client(&self, client: &str, key: &str) -> Result<()> {
        crate::agent_clients::AgentClientService::new(self.pool.clone(), self.clients.clone())
            .for_creation_key(client, key)
            .await?
            .ok_or_else(|| Error::Unsupported("Client configuration unavailable".into()))?;
        Ok(())
    }

    pub async fn launch_client_unavailable_reason(
        &self,
        client: &str,
        key: &str,
        use_yolo: bool,
    ) -> Result<Option<String>> {
        match self
            .validate_launch_client(client, key)
            .await
            .and_then(|()| self.validate_yolo(client, key, use_yolo))
        {
            Ok(()) => Ok(None),
            Err(
                error @ (Error::Unsupported(_)
                | Error::Unavailable(_)
                | Error::StateConflict(_)
                | Error::Conflict { .. }
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
            self.validate_yolo(client, key, profile.use_yolo)?;
        }
        Ok(ProfileLaunchConfig {
            client_type: profile.client_type,
            client_key: profile.client_key,
            model: profile.model,
            reasoning_effort: profile.reasoning_effort,
            mode: if profile.use_yolo {
                pontia_core::SessionMode::Yolo
            } else {
                pontia_core::SessionMode::Default
            },
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
                .launch_client_unavailable_reason(&client_type, &client_key, profile.use_yolo)
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
                reasoning_effort: profile.reasoning_effort,
            });
        }
        Ok(launchable)
    }
}

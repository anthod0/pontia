use super::AgentProfileService;
use crate::client_contract::ResolvedClientProfile;
use pontia_core::{Error, Result};

impl AgentProfileService {
    pub async fn resolve_for_client(
        &self,
        client_type: &str,
        id: Option<&str>,
        version: Option<&str>,
    ) -> Result<Option<ResolvedClientProfile>> {
        let Some(policy) = self
            .clients
            .get(client_type)
            .and_then(|client| client.profile.as_ref())
        else {
            return Ok(None);
        };
        let Some(id) = id else {
            if version.is_some() {
                return Err(Error::Domain(
                    "execution_profile_version requires execution_profile_id".into(),
                ));
            }
            return Ok(None);
        };
        let profile = match version {
            Some(version) => self.get_version(id, version).await?,
            None => self.get_latest(id).await?,
        }
        .ok_or_else(|| {
            Error::NotFound(format!(
                "agent profile {id}@{} not found",
                version.unwrap_or("latest")
            ))
        })?;
        if !profile.active {
            return Err(Error::Domain(format!(
                "agent profile {id}@{} is archived",
                profile.version
            )));
        }
        if !profile
            .supported_client_types
            .iter()
            .any(|client| client == client_type)
        {
            return Err(Error::Domain(format!(
                "agent profile {id}@{} does not declare {client_type} support",
                profile.version
            )));
        }
        policy.validate_templates(
            profile.system_prompt_template.as_deref(),
            profile.turn_prompt_template.as_deref(),
        )?;
        Ok(Some(ResolvedClientProfile {
            version: profile.version.clone(),
            binding: policy.bind(&profile)?,
        }))
    }
}

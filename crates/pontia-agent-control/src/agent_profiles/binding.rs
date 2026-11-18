use super::AgentProfileService;
use crate::client_contract::ResolvedClientProfile;
use pontia_core::{Error, Result};
use serde_json::Value;

impl AgentProfileService {
    /// Resolve and pin a Profile version before creating a Session. Instruction binding
    /// still goes through `resolve_for_client` with this version at Session creation.
    pub async fn launch_defaults(
        &self,
        id: &str,
        version: Option<&str>,
    ) -> Result<super::ProfileLaunchDefaults> {
        let profile = match version {
            Some(version) => self.get_version(id, version).await?,
            None => self.get_latest(id).await?,
        }
        .ok_or_else(|| Error::NotFound(format!("agent profile {id} not found")))?;
        if !profile.active {
            return Err(Error::Domain(format!("agent profile {id} is archived")));
        }
        let (Some(client_type), Some(client_key), Some(model)) = (
            profile.default_client_type,
            profile.default_client_key,
            profile.default_model,
        ) else {
            return Err(Error::Domain(format!(
                "agent profile {id} has no launch defaults"
            )));
        };
        crate::agent_clients::AgentClientService::new(self.pool.clone(), self.clients.clone())
            .resolve(&client_type, &client_key)
            .await?;
        Ok(super::ProfileLaunchDefaults {
            profile_id: profile.profile_id,
            version: profile.version,
            client_type,
            client_key,
            model,
        })
    }

    pub async fn resolve_for_client(
        &self,
        client_type: &str,
        id: Option<&str>,
        version: Option<&str>,
        additional_instructions: Option<&str>,
    ) -> Result<Option<ResolvedClientProfile>> {
        if additional_instructions.is_some_and(|instructions| instructions.trim().is_empty()) {
            return Err(Error::Domain(
                "additional_instructions cannot be empty; omit it for no additional instructions"
                    .into(),
            ));
        }
        let Some(policy) = self
            .clients
            .get(client_type)
            .and_then(|client| client.profile.as_ref())
        else {
            if additional_instructions.is_some() {
                return Err(Error::CapabilityUnavailable(
                    "Client cannot inject additional instructions".into(),
                ));
            }
            return Ok(None);
        };
        let explicit = if policy.supports_explicit() {
            match id {
                Some(id) => {
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
                    Some(profile)
                }
                None => {
                    if version.is_some() {
                        return Err(Error::Domain(
                            "execution_profile_version requires execution_profile_id".into(),
                        ));
                    }
                    None
                }
            }
        } else {
            None
        };
        let builtin = if policy.inject_builtin() {
            self.list_latest().await?.into_iter().find(|profile| {
                profile.metadata.get("builtin") == Some(&Value::Bool(true))
                    && profile
                        .supported_client_types
                        .iter()
                        .any(|client| client == client_type)
            })
        } else {
            None
        };
        let selected_version = explicit.as_ref().map(|profile| profile.version.clone());
        let mut profile = match (&explicit, &builtin) {
            (Some(profile), _) | (None, Some(profile)) => profile.clone(),
            (None, None) => {
                if additional_instructions.is_some() {
                    return Err(Error::CapabilityUnavailable(
                        "Client has no Profile for instruction binding".into(),
                    ));
                }
                return Ok(None);
            }
        };
        if let (Some(builtin), Some(explicit)) = (&builtin, &explicit)
            && builtin.profile_id != explicit.profile_id
        {
            profile.system_prompt_template = match (
                &builtin.system_prompt_template,
                &explicit.system_prompt_template,
            ) {
                (Some(default), Some(selected)) => Some(format!("{default}\n\n{selected}")),
                (Some(default), None) => Some(default.clone()),
                (None, selected) => selected.clone(),
            };
        }
        if let Some(instructions) = additional_instructions {
            profile.system_prompt_template = Some(match profile.system_prompt_template {
                Some(prompt) if !prompt.is_empty() => format!("{prompt}\n\n{instructions}"),
                _ => instructions.to_owned(),
            });
        }
        Ok(Some(ResolvedClientProfile {
            version: selected_version,
            binding: policy.bind(&profile)?,
        }))
    }

    /// Session content is read only from its creation event, including during recovery.
    pub async fn session_binding(&self, session: &str) -> Result<Option<Value>> {
        let snapshot: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(payload,'$.execution_profile_binding') FROM events WHERE session_id=? AND event_type='session.created'",
        ).bind(session).fetch_optional(&self.pool).await?.flatten();
        snapshot
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }
}

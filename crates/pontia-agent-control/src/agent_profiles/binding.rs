use super::AgentProfileService;
use crate::client_contract::{ProfileBindingInput, ResolvedClientProfile};
use pontia_core::{Error, Result};
use serde_json::Value;

pub fn standalone_session_instructions() -> &'static str {
    include_str!("standalone_session_instructions.md").trim()
}

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
        include_standalone_session_instructions: bool,
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
        let explicit = match id {
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
        };
        let standalone_instructions = (include_standalone_session_instructions
            && policy.inject_standalone_session_instructions())
        .then(standalone_session_instructions)
        .filter(|instructions| !instructions.is_empty());
        let instructions_required = additional_instructions.is_some()
            || explicit
                .as_ref()
                .is_some_and(|profile| profile.system_prompt_template.is_some());
        let mut parts = Vec::new();
        if let Some(standalone) = standalone_instructions {
            parts.push(standalone);
        }
        if let Some(selected) = explicit
            .as_ref()
            .and_then(|profile| profile.system_prompt_template.as_deref())
            .filter(|prompt| !prompt.is_empty())
        {
            parts.push(selected);
        }
        if let Some(instructions) = additional_instructions {
            parts.push(instructions);
        }
        let system_prompt = (!parts.is_empty()).then(|| parts.join("\n\n"));
        if explicit.is_none() && system_prompt.is_none() {
            return Ok(None);
        }
        let selected_version = explicit.as_ref().map(|profile| profile.version.clone());
        let binding = policy.bind(&ProfileBindingInput {
            profile_id: explicit.as_ref().map(|profile| profile.profile_id.as_str()),
            version: selected_version.as_deref(),
            system_prompt: system_prompt.as_deref(),
            instructions_required,
        })?;
        Ok(Some(ResolvedClientProfile {
            version: selected_version,
            binding,
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

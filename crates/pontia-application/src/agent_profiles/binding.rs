use super::AgentProfileService;
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// Immutable execution content, committed with the Session creation event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodexProfileBinding {
    pub contract_version: u32,
    pub profile_id: String,
    pub version: String,
    pub system_prompt: Option<String>,
}

pub(super) fn validate_codex_templates(system: Option<&str>, turn: Option<&str>) -> Result<()> {
    if turn.is_some() {
        return Err(Error::Domain(
            "Codex does not support turn_prompt_template across Dashboard and native TUI; omit this field".into(),
        ));
    }
    if let Some(system) = system {
        if system.trim().is_empty() {
            return Err(Error::Domain(
                "system_prompt_template cannot be empty; omit it for no profile instructions"
                    .into(),
            ));
        }
        if system.contains("{{") || system.contains("}}") {
            return Err(Error::Domain("Codex system_prompt_template supports static instructions only; template placeholders are unsupported".into()));
        }
    }
    Ok(())
}

impl AgentProfileService {
    /// Records an adapter-confirmed native configuration against the fixed binding.
    pub async fn confirm_codex_configuration(&self, session: &str, native_key: &str) -> Result<()> {
        if self.codex_binding(session).await?.is_none() {
            return Err(Error::StateConflict(
                "Session has no Codex Profile to configure".into(),
            ));
        }
        let updated = sqlx::query(
            "UPDATE runtime_bindings SET adapter_details=json_set(adapter_details,'$.codex_profile_thread',?) WHERE session_id=? AND EXISTS(SELECT 1 FROM agent_bindings a WHERE a.session_id=runtime_bindings.session_id AND a.client_type='codex' AND a.client_session_key=?)",
        ).bind(native_key).bind(session).bind(native_key).execute(&self.pool).await?;
        if updated.rows_affected() != 1 {
            return Err(Error::StateConflict(
                "Codex Profile configuration does not match the Session's native binding".into(),
            ));
        }
        Ok(())
    }

    pub async fn codex_profile_for_thread(
        &self,
        native_key: &str,
    ) -> Result<Option<CodexProfileBinding>> {
        let Some(binding) = crate::AgentBindingService::new(self.pool.clone())
            .binding_for_client_session("codex", native_key)
            .await?
        else {
            return Ok(None);
        };
        self.configured_codex_binding(&binding.session_id).await
    }

    pub async fn resolve_codex_profile(
        &self,
        id: Option<&str>,
        version: Option<&str>,
    ) -> Result<Option<CodexProfileBinding>> {
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
            .any(|client| client == "codex")
        {
            return Err(Error::Domain(format!(
                "agent profile {id}@{} does not declare Codex support",
                profile.version
            )));
        }
        validate_codex_templates(
            profile.system_prompt_template.as_deref(),
            profile.turn_prompt_template.as_deref(),
        )?;
        Ok(Some(CodexProfileBinding {
            contract_version: 1,
            profile_id: profile.profile_id,
            version: profile.version,
            system_prompt: profile.system_prompt_template,
        }))
    }

    /// Reads the creation-time snapshot, never the current mutable Profile row.
    /// Legacy bindings cannot be silently upgraded to a different native prompt.
    pub async fn codex_binding(&self, session: &str) -> Result<Option<CodexProfileBinding>> {
        let (id, version): (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT execution_profile_id,execution_profile_version FROM sessions WHERE session_id=?",
        ).bind(session).fetch_one(&self.pool).await?;
        let Some(_) = id else {
            if version.is_some() {
                return Err(unverified());
            }
            return Ok(None);
        };
        let snapshot: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(payload,'$.execution_profile_binding') FROM events WHERE session_id=? AND event_type='session.created'",
        ).bind(session).fetch_optional(&self.pool).await?.flatten();
        let snapshot: CodexProfileBinding = snapshot
            .and_then(|value| serde_json::from_str(&value).ok())
            .ok_or_else(unverified)?;
        if snapshot.contract_version != 1 {
            return Err(unverified());
        }
        Ok(Some(snapshot))
    }

    pub async fn configured_codex_binding(
        &self,
        session: &str,
    ) -> Result<Option<CodexProfileBinding>> {
        let profile = self.codex_binding(session).await?;
        if profile.is_some() {
            let configured: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM agent_bindings a JOIN runtime_bindings r USING(session_id) WHERE a.session_id=? AND json_extract(r.adapter_details,'$.codex_profile_thread')=a.client_session_key)",
            ).bind(session).fetch_one(&self.pool).await?;
            if !configured {
                return Err(unverified());
            }
        }
        Ok(profile)
    }
}

fn unverified() -> Error {
    Error::StateConflict("Codex Profile binding is unverified; create a new Session with a supported Profile. Existing native instructions will not be changed automatically".into())
}

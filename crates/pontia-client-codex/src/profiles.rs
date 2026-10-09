use pontia_application::{
    AgentBindingService, ExecutionProfileView, client_contract::ClientProfile,
};
use sqlx::SqlitePool;

pub struct CodexProfiles {
    pool: SqlitePool,
}
impl CodexProfiles {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

pub(crate) struct CodexProfilePolicy;
impl ClientProfile for CodexProfilePolicy {
    fn validate_templates(&self, system: Option<&str>, turn: Option<&str>) -> Result<()> {
        validate_codex_templates(system, turn)
    }
    fn bind(&self, profile: &ExecutionProfileView) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(CodexProfileBinding {
            contract_version: 1,
            profile_id: profile.profile_id.clone(),
            version: profile.version.clone(),
            system_prompt: profile.system_prompt_template.clone(),
        })?)
    }
}
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

fn validate_codex_templates(system: Option<&str>, turn: Option<&str>) -> Result<()> {
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

impl CodexProfiles {
    /// Confirms that the adapter-bound native thread carries the fixed creation-time Profile.
    pub async fn confirm_codex_configuration(&self, session: &str, native_key: &str) -> Result<()> {
        if self.codex_binding(session).await?.is_none() {
            return Err(unverified());
        }
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session)
            .await?;
        match binding {
            Some(binding)
                if binding.client_type == "codex" && binding.client_session_key == native_key =>
            {
                Ok(())
            }
            _ => Err(unverified()),
        }
    }

    pub async fn codex_profile_for_thread(
        &self,
        native_key: &str,
    ) -> Result<Option<CodexProfileBinding>> {
        let Some(binding) = AgentBindingService::new(self.pool.clone())
            .binding_for_client_session("codex", native_key)
            .await?
        else {
            return Ok(None);
        };
        self.configured_codex_binding(&binding.session_id).await
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
        let Some(profile) = profile else {
            return Ok(None);
        };
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session)
            .await?;
        match binding {
            Some(binding) if binding.client_type == "codex" => Ok(Some(profile)),
            _ => Err(unverified()),
        }
    }
}

fn unverified() -> Error {
    Error::StateConflict("Codex Profile binding is unverified; create a new Session with a supported Profile. Existing native instructions will not be changed automatically".into())
}

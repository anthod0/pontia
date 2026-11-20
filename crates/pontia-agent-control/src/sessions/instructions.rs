use crate::{client_contract::InstructionsBindingInput, clients::ClientRegistry};
use pontia_core::{Error, Result};
use serde_json::Value;
use sqlx::SqlitePool;

pub fn standalone_session_instructions() -> &'static str {
    include_str!("standalone_session_instructions.md").trim()
}

pub fn validate_static_instructions(system: Option<&str>) -> Result<()> {
    if let Some(system) = system {
        if system.trim().is_empty() {
            return Err(Error::Domain("system_instructions cannot be empty".into()));
        }
        if system.contains("{{") || system.contains("}}") {
            return Err(Error::Domain(
                "system_instructions must be static; template placeholders are unsupported".into(),
            ));
        }
    }
    Ok(())
}

#[derive(Clone)]
pub struct SessionInstructionsService {
    pool: SqlitePool,
    clients: ClientRegistry,
}

impl SessionInstructionsService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            clients: Default::default(),
        }
    }

    pub fn with_clients(mut self, clients: ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub fn resolve(
        &self,
        client_type: &str,
        system_instructions: Option<&str>,
        include_standalone_session_instructions: bool,
    ) -> Result<Option<Value>> {
        validate_static_instructions(system_instructions)?;
        let Some(policy) = self
            .clients
            .get(client_type)
            .and_then(|client| client.instructions.as_ref())
        else {
            if system_instructions.is_some() {
                return Err(Error::CapabilityUnavailable(
                    "Client cannot inject system instructions".into(),
                ));
            }
            return Ok(None);
        };
        let mut parts = Vec::new();
        if include_standalone_session_instructions
            && policy.inject_standalone_session_instructions()
        {
            parts.push(standalone_session_instructions());
        }
        if let Some(instructions) = system_instructions {
            parts.push(instructions);
        }
        if parts.is_empty() {
            return Ok(None);
        }
        let system_prompt = parts.join("\n\n");
        policy.validate_instructions(Some(&system_prompt))?;
        policy
            .bind(&InstructionsBindingInput {
                system_prompt: Some(&system_prompt),
                instructions_required: system_instructions.is_some(),
            })
            .map(Some)
    }

    pub async fn session_instructions(&self, session: &str) -> Result<Option<Value>> {
        let snapshot: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(payload,'$.session_instructions') FROM events WHERE session_id=? AND event_type='session.created'",
        ).bind(session).fetch_optional(&self.pool).await?.flatten();
        snapshot
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }
}

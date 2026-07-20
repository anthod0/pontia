use super::*;
use pontia_storage_sqlite::{
    models::agent_bindings::AgentBindingRow,
    repositories::{
        agent_bindings::{AgentBindingUpsertRecord, SqliteAgentBindingRepository},
        runtime_bindings::SqliteRuntimeBindingRepository,
        sessions::SqliteSessionRepository,
        turns::SqliteTurnRepository,
    },
};
use sqlx::Row;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentBinding {
    pub id: String,
    pub session_id: String,
    pub client_type: String,
    pub launch_cwd: String,
    pub client_session_key: String,
    pub metadata: Value,
    pub discovered: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpsertAgentBindingRequest {
    pub session_id: String,
    pub client_type: String,
    pub launch_cwd: String,
    pub client_session_key: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentBindingSessionContext {
    pub session_id: String,
    pub session_state: String,
    pub client_type: String,
    pub client_session_key: String,
    pub runtime_instance_id: Option<String>,
    pub internal_event_url: String,
    pub binding_metadata: Value,
    pub runtime_metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentBindingCurrentTurn {
    pub session_id: String,
    pub turn_id: String,
    pub client_type: String,
    pub client_session_key: String,
    pub runtime_instance_id: Option<String>,
    pub internal_event_url: String,
    pub binding_metadata: Value,
    pub runtime_metadata: Value,
}

#[derive(Clone)]
pub struct AgentBindingService {
    pool: SqlitePool,
}

impl AgentBindingService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_binding(&self, request: UpsertAgentBindingRequest) -> Result<AgentBinding> {
        let record = agent_binding_upsert_record(request)?;
        let row = SqliteAgentBindingRepository::new(self.pool.clone())
            .upsert_binding(record)
            .await?;
        agent_binding_from_row(row)
    }

    pub async fn binding_for_client_session(
        &self,
        client_type: &str,
        client_session_key: &str,
    ) -> Result<Option<AgentBinding>> {
        let row = SqliteAgentBindingRepository::new(self.pool.clone())
            .binding_for_client_session(client_type, client_session_key)
            .await?;

        let binding = row.map(agent_binding_from_row).transpose()?;
        if let Some(binding) = binding.as_ref() {
            self.ensure_binding_agrees_with_session_and_runtime(binding)
                .await?;
        }
        Ok(binding)
    }

    pub async fn session_context_for_client_session(
        &self,
        client_type: &str,
        client_session_key: &str,
    ) -> Result<Option<AgentBindingSessionContext>> {
        let Some(binding) = self
            .binding_for_client_session(client_type, client_session_key)
            .await?
        else {
            return Ok(None);
        };

        let Some(row) = sqlx::query(
            r#"SELECT s.state AS session_state,
                      r.runtime_instance_id,
                      r.metadata AS runtime_metadata
               FROM sessions s
               JOIN runtime_bindings r ON r.session_id = s.session_id
               WHERE s.session_id = ?"#,
        )
        .bind(&binding.session_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let runtime_metadata = runtime_metadata_from_row(&row)?;
        let internal_event_url = internal_event_url(&runtime_metadata);
        Ok(Some(AgentBindingSessionContext {
            session_id: binding.session_id,
            session_state: row.try_get("session_state")?,
            client_type: binding.client_type,
            client_session_key: binding.client_session_key,
            runtime_instance_id: row.try_get("runtime_instance_id")?,
            internal_event_url,
            binding_metadata: binding.metadata,
            runtime_metadata,
        }))
    }

    pub async fn current_turn_for_client_session(
        &self,
        client_type: &str,
        client_session_key: &str,
    ) -> Result<Option<AgentBindingCurrentTurn>> {
        let Some(binding) = self
            .binding_for_client_session(client_type, client_session_key)
            .await?
        else {
            return Ok(None);
        };

        let Some(row) = sqlx::query(
            r#"SELECT r.runtime_instance_id,
                      r.metadata AS runtime_metadata
               FROM runtime_bindings r
               WHERE r.session_id = ?"#,
        )
        .bind(&binding.session_id)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let Some(active_turn) = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(&binding.session_id)
            .await?
        else {
            return Ok(None);
        };
        let turn_id = active_turn.turn_id;
        let runtime_metadata = runtime_metadata_from_row(&row)?;
        let internal_event_url = internal_event_url(&runtime_metadata);

        Ok(Some(AgentBindingCurrentTurn {
            session_id: binding.session_id,
            turn_id,
            client_type: binding.client_type,
            client_session_key: binding.client_session_key,
            runtime_instance_id: row.try_get("runtime_instance_id")?,
            internal_event_url,
            binding_metadata: binding.metadata,
            runtime_metadata,
        }))
    }

    pub async fn binding_for_session(&self, session_id: &str) -> Result<Option<AgentBinding>> {
        let row = SqliteAgentBindingRepository::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?;

        let binding = row.map(agent_binding_from_row).transpose()?;
        if let Some(binding) = binding.as_ref() {
            self.ensure_binding_agrees_with_session_and_runtime(binding)
                .await?;
        }
        Ok(binding)
    }

    pub async fn mark_discovered(&self, binding_id: &str) -> Result<()> {
        SqliteAgentBindingRepository::new(self.pool.clone())
            .mark_discovered(binding_id)
            .await
    }

    async fn ensure_binding_agrees_with_session_and_runtime(
        &self,
        binding: &AgentBinding,
    ) -> Result<()> {
        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(&binding.session_id)
            .await?
            .ok_or_else(|| {
                Error::StateConflict(format!(
                    "Agent binding {} references missing Session {}",
                    binding.id, binding.session_id
                ))
            })?;
        if session.client_type != binding.client_type {
            return Err(Error::StateConflict(format!(
                "Session {} client type does not match its Agent binding",
                binding.session_id
            )));
        }

        let Some(runtime_metadata) = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .metadata(&binding.session_id)
            .await?
        else {
            return Ok(());
        };
        if runtime_binding_identity_disagrees(&runtime_metadata, &binding.client_session_key)? {
            return Err(Error::StateConflict(format!(
                "Session {} Runtime binding client identity does not match its Agent binding",
                binding.session_id
            )));
        }
        Ok(())
    }
}

fn runtime_metadata_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Value> {
    let runtime_metadata = row
        .try_get::<Option<String>, _>("runtime_metadata")?
        .unwrap_or_else(|| "{}".to_string());
    Ok(serde_json::from_str(&runtime_metadata)?)
}

fn internal_event_url(runtime_metadata: &Value) -> String {
    runtime_metadata
        .get("internal_event_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("http://127.0.0.1:8080/internal/v1/events")
        .to_string()
}

pub(crate) fn runtime_binding_identity_disagrees(
    runtime_metadata: &str,
    client_session_key: &str,
) -> Result<bool> {
    let runtime_metadata: Value = serde_json::from_str(runtime_metadata)?;
    Ok(runtime_metadata
        .get("client_session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some_and(|runtime_client_session_key| runtime_client_session_key != client_session_key))
}

pub(crate) async fn register_agent_binding_for_ready_event_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &DomainEvent,
) -> Result<Option<AgentBinding>> {
    if event.event_type != EventType::SessionReady || event.source != EventSource::AgentClient {
        return Ok(None);
    }

    let Some(client_session_key) = event
        .payload
        .get("client_session_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
    else {
        return Ok(None);
    };

    let runtime_metadata = SqliteRuntimeBindingRepository::metadata_in_tx(tx, &event.session_id)
        .await?
        .ok_or_else(|| {
            Error::Domain(format!(
                "session {} runtime binding missing",
                event.session_id
            ))
        })?;
    let runtime_metadata: Value = serde_json::from_str(&runtime_metadata)?;
    let launch_cwd = runtime_metadata
        .get("workspace")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            Error::Domain(format!(
                "session {} runtime binding missing workspace launch_cwd",
                event.session_id
            ))
        })?
        .to_string();

    let metadata = diagnostic_metadata_from_ready_payload(&event.payload);
    let binding = upsert_agent_binding_in_tx(
        tx,
        UpsertAgentBindingRequest {
            session_id: event.session_id.clone(),
            client_type: event.client_type.clone(),
            launch_cwd,
            client_session_key,
            metadata,
        },
    )
    .await?;

    Ok(Some(binding))
}

pub(crate) async fn upsert_agent_binding_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    request: UpsertAgentBindingRequest,
) -> Result<AgentBinding> {
    let record = agent_binding_upsert_record(request)?;
    let row = SqliteAgentBindingRepository::upsert_binding_in_tx(tx, record).await?;
    agent_binding_from_row(row)
}

fn agent_binding_upsert_record(
    request: UpsertAgentBindingRequest,
) -> Result<AgentBindingUpsertRecord> {
    Ok(AgentBindingUpsertRecord {
        id: pontia_core::ids::new_agent_binding_id().to_string(),
        session_id: request.session_id,
        client_type: request.client_type,
        launch_cwd: request.launch_cwd,
        client_session_key: request.client_session_key,
        metadata: serde_json::to_string(&request.metadata)?,
    })
}

fn agent_binding_from_row(row: AgentBindingRow) -> Result<AgentBinding> {
    Ok(AgentBinding {
        id: row.id,
        session_id: row.session_id,
        client_type: row.client_type,
        launch_cwd: row.launch_cwd,
        client_session_key: row.client_session_key,
        metadata: serde_json::from_str(&row.metadata)?,
        discovered: row.discovered,
    })
}

fn diagnostic_metadata_from_ready_payload(payload: &Value) -> Value {
    let mut metadata = serde_json::Map::new();
    for key in ["client_session_file", "client_session_dir", "client_cwd"] {
        if let Some(value) = payload.get(key) {
            metadata.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(metadata)
}

use super::*;
use pontia_storage_sqlite::{
    models::agent_bindings::AgentBindingRow,
    repositories::{
        agent_bindings::{AgentBindingUpsertRecord, SqliteAgentBindingRepository},
        runtime_bindings::SqliteRuntimeBindingRepository,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
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

    pub async fn primary_binding_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<AgentBinding>> {
        let row = SqliteAgentBindingRepository::new(self.pool.clone())
            .primary_binding_for_session(session_id)
            .await?;

        row.map(agent_binding_from_row).transpose()
    }

    pub async fn mark_discovered(&self, binding_id: &str) -> Result<()> {
        SqliteAgentBindingRepository::new(self.pool.clone())
            .mark_discovered(binding_id)
            .await
    }
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

async fn upsert_agent_binding_in_tx(
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

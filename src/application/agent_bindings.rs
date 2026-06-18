use super::*;
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;

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
        let mut tx = self.pool.begin().await?;
        let binding = upsert_agent_binding_in_tx(&mut tx, request).await?;
        tx.commit().await?;
        Ok(binding)
    }

    pub async fn primary_binding_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<AgentBinding>> {
        let row = sqlx::query(
            r#"SELECT id, session_id, client_type, launch_cwd, client_session_key, metadata, discovered
               FROM agent_bindings
               WHERE session_id = ?
               ORDER BY created_at, id
               LIMIT 1"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_agent_binding).transpose()
    }

    pub async fn mark_discovered(&self, binding_id: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE agent_bindings
               SET discovered = TRUE,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE id = ? AND discovered = FALSE"#,
        )
        .bind(binding_id)
        .execute(&self.pool)
        .await?;
        Ok(())
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
    let id = crate::ids::new_agent_binding_id().to_string();
    let metadata = serde_json::to_string(&request.metadata)?;
    let row = sqlx::query(
        r#"INSERT INTO agent_bindings
           (id, session_id, client_type, launch_cwd, client_session_key, metadata)
           VALUES (?, ?, ?, ?, ?, ?)
           ON CONFLICT(session_id, client_type, client_session_key) DO UPDATE SET
               metadata = excluded.metadata,
               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
           RETURNING id, session_id, client_type, launch_cwd, client_session_key, metadata, discovered"#,
    )
    .bind(id)
    .bind(request.session_id)
    .bind(request.client_type)
    .bind(request.launch_cwd)
    .bind(request.client_session_key)
    .bind(metadata)
    .fetch_one(&mut **tx)
    .await?;

    row_to_agent_binding(row)
}

fn row_to_agent_binding(row: sqlx::sqlite::SqliteRow) -> Result<AgentBinding> {
    let metadata: String = row.try_get("metadata")?;
    Ok(AgentBinding {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        client_type: row.try_get("client_type")?,
        launch_cwd: row.try_get("launch_cwd")?,
        client_session_key: row.try_get("client_session_key")?,
        metadata: serde_json::from_str(&metadata)?,
        discovered: row.try_get("discovered")?,
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

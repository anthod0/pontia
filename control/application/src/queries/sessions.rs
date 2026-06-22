use super::*;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;

impl ExternalQueryService {
    pub async fn list_sessions(&self) -> Result<Vec<SessionView>> {
        let repository = SqliteSessionRepository::new(self.pool.clone());
        let rows = repository.list_sessions().await?;

        let mut sessions = rows
            .into_iter()
            .map(session_row_to_view)
            .collect::<Result<Vec<_>>>()?;
        for session in &mut sessions {
            self.enrich_session_view(session).await?;
        }
        Ok(sessions)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionView>> {
        let repository = SqliteSessionRepository::new(self.pool.clone());
        let Some(row) = repository.get_session(session_id).await? else {
            return Ok(None);
        };
        let mut session = session_row_to_view(row)?;
        self.enrich_session_view(&mut session).await?;
        Ok(Some(session))
    }

    async fn enrich_session_view(&self, session: &mut SessionView) -> Result<()> {
        let repository = SqliteSessionRepository::new(self.pool.clone());
        let row = repository
            .get_runtime_binding_metadata(&session.session_id)
            .await?;

        if let Some(row) = row {
            let metadata: Value = serde_json::from_str(&row.metadata)?;
            if let Some(capabilities) = metadata.get("capabilities") {
                session.capabilities = serde_json::from_value(capabilities.clone())?;
            } else if let Some(capabilities) =
                legacy_binding_capabilities(&session.client_type, &metadata)
            {
                session.capabilities = capabilities;
            }
        }

        Ok(())
    }
}

fn legacy_binding_capabilities(client_type: &str, metadata: &Value) -> Option<SessionCapabilities> {
    let client_spec = agent_clients::get_client_spec(client_type)?;
    let mut capabilities = client_spec.capabilities.clone();
    if client_spec.tmux_runtime().is_some() {
        let writable = non_empty_json_string(metadata, "tmux_socket_path").is_some()
            && non_empty_json_string(metadata, "tmux_pane_id").is_some();
        capabilities.accept_task = writable;
        capabilities.interrupt = writable;
    }
    Some(capabilities)
}

fn non_empty_json_string<'a>(metadata: &'a Value, key: &str) -> Option<&'a str> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

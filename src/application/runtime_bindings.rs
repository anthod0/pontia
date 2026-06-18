use super::*;
use crate::runtime::{GenericRuntimeManager, configured_internal_event_url};

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RuntimeBindingUpsertRequest {
    pub client_type: String,
    pub client_session_key: String,
    pub client_session_file: Option<String>,
    pub client_session_dir: Option<String>,
    pub client_cwd: Option<String>,
    pub launch_cwd: Option<String>,
    pub runtime_instance_id: String,
    pub start_command: Option<String>,
    pub tmux: Option<RuntimeBindingTmuxRequest>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct RuntimeBindingTmuxRequest {
    pub socket_path: Option<String>,
    pub session_id: Option<String>,
    pub session_name: Option<String>,
    pub window_id: Option<String>,
    pub window_index: Option<i64>,
    pub pane_id: Option<String>,
    pub pane_index: Option<i64>,
    pub pane_current_path: Option<String>,
}

#[derive(Clone)]
pub struct RuntimeBindingUpsertService {
    pool: SqlitePool,
}

impl RuntimeBindingUpsertService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert(&self, request: RuntimeBindingUpsertRequest) -> Result<Value> {
        validate_required("client_type", &request.client_type)?;
        validate_required("client_session_key", &request.client_session_key)?;
        validate_required("runtime_instance_id", &request.runtime_instance_id)?;
        let client_spec =
            agent_clients::get_client_spec(&request.client_type).ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", request.client_type))
            })?;
        let runtime_kind = client_spec.runtime_binding_kind().ok_or_else(|| {
            Error::Domain(format!(
                "runtime binding upsert does not support client_type {}",
                request.client_type
            ))
        })?;

        let launch_cwd = request
            .launch_cwd
            .as_deref()
            .or(request.client_cwd.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| Error::Domain("launch_cwd or client_cwd is required".to_string()))?;
        let workspace = upsert_workspace(&self.pool, launch_cwd).await?;

        let session_id = match self
            .session_id_for_client_session(&request.client_type, &request.client_session_key)
            .await?
        {
            Some(session_id) => session_id,
            None => self.create_bound_session(&request, &workspace).await?,
        };

        sqlx::query("UPDATE sessions SET workspace_ref = ?, workspace_id = ? WHERE session_id = ?")
            .bind(&workspace.canonical_path)
            .bind(&workspace.workspace_id)
            .bind(&session_id)
            .execute(&self.pool)
            .await?;

        let runtime_dir = pontia_runtime_dir(&session_id)?;
        std::fs::create_dir_all(&runtime_dir)?;
        let current_turn_file = runtime_dir.join("current-turn.json").display().to_string();
        let internal_event_url = configured_internal_event_url()
            .unwrap_or_else(|| "http://127.0.0.1:8080/internal/v1/events".to_string());
        let capabilities = capabilities_for_tmux(client_spec, request.tmux.as_ref());
        let last_seen_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .map_err(|err| Error::Domain(format!("failed to format timestamp: {err}")))?;
        let tmux_socket_path = request
            .tmux
            .as_ref()
            .and_then(|tmux| non_empty(tmux.socket_path.as_deref()));
        let tmux_pane_id = request
            .tmux
            .as_ref()
            .and_then(|tmux| non_empty(tmux.pane_id.as_deref()));

        let metadata = binding_metadata(
            &request,
            &workspace.canonical_path,
            &internal_event_url,
            &current_turn_file,
            &runtime_dir.display().to_string(),
            &capabilities,
        );

        sqlx::query(
            r#"INSERT INTO runtime_bindings (
                   session_id,
                   runtime_kind,
                   runtime_instance_id,
                   start_command,
                   launch_cwd,
                   last_seen_at,
                   tmux_socket_path,
                   tmux_pane_id,
                   metadata
               )
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_instance_id = excluded.runtime_instance_id,
                   start_command = excluded.start_command,
                   launch_cwd = excluded.launch_cwd,
                   last_seen_at = excluded.last_seen_at,
                   tmux_socket_path = excluded.tmux_socket_path,
                   tmux_pane_id = excluded.tmux_pane_id,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(&session_id)
        .bind(runtime_kind)
        .bind(&request.runtime_instance_id)
        .bind(non_empty(request.start_command.as_deref()))
        .bind(&workspace.canonical_path)
        .bind(&last_seen_at)
        .bind(tmux_socket_path.as_deref())
        .bind(tmux_pane_id.as_deref())
        .bind(serde_json::to_string(&metadata)?)
        .execute(&self.pool)
        .await?;

        if let (Some(socket_path), Some(pane_id)) =
            (tmux_socket_path.as_deref(), tmux_pane_id.as_deref())
        {
            let _ = GenericRuntimeManager.mark_tmux_pane_for_session(
                socket_path,
                pane_id,
                &session_id,
                &request.runtime_instance_id,
            );
        }

        AgentBindingService::new(self.pool.clone())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session_id.clone(),
                client_type: request.client_type.clone(),
                launch_cwd: workspace.canonical_path.clone(),
                client_session_key: request.client_session_key.clone(),
                metadata: agent_binding_metadata(&request),
            })
            .await?;

        Ok(json!({
            "session": {
                "session_id": session_id,
            },
            "runtime": {
                "runtime_instance_id": request.runtime_instance_id,
                "internal_event_url": internal_event_url,
                "current_turn_file": current_turn_file,
                "capabilities": capabilities,
            }
        }))
    }

    async fn session_id_for_client_session(
        &self,
        client_type: &str,
        client_session_key: &str,
    ) -> Result<Option<String>> {
        sqlx::query_scalar(
            r#"SELECT session_id
               FROM agent_bindings
               WHERE client_type = ? AND client_session_key = ?
               ORDER BY updated_at DESC, id DESC
               LIMIT 1"#,
        )
        .bind(client_type)
        .bind(client_session_key)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn create_bound_session(
        &self,
        request: &RuntimeBindingUpsertRequest,
        workspace: &WorkspaceRecord,
    ) -> Result<String> {
        let session_id = new_session_id().to_string();
        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::AgentClient,
                request.client_type.clone(),
                EventType::SessionCreated,
                json!({
                    "workspace": workspace.canonical_path,
                    "metadata": {
                        "created_by": "runtime_binding_upsert",
                        "client_session_key": request.client_session_key,
                    }
                }),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::AgentClient,
                request.client_type.clone(),
                EventType::SessionStarting,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::AgentClient,
                request.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        Ok(session_id)
    }
}

fn validate_required(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::Domain(format!("{field} is required")));
    }
    Ok(())
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn capabilities_for_tmux(
    client_spec: &agent_clients::AgentClientSpec,
    tmux: Option<&RuntimeBindingTmuxRequest>,
) -> SessionCapabilities {
    let writable = tmux.is_some_and(|tmux| {
        non_empty(tmux.socket_path.as_deref()).is_some()
            && non_empty(tmux.pane_id.as_deref()).is_some()
    });
    let mut capabilities: SessionCapabilities = client_spec.capabilities.clone().into();
    capabilities.accept_task = writable;
    capabilities.interrupt = writable;
    capabilities
}

fn binding_metadata(
    request: &RuntimeBindingUpsertRequest,
    launch_cwd: &str,
    internal_event_url: &str,
    current_turn_file: &str,
    runtime_dir: &str,
    capabilities: &SessionCapabilities,
) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "client_session_key".to_string(),
        json!(request.client_session_key),
    );
    insert_optional(
        &mut metadata,
        "client_session_file",
        &request.client_session_file,
    );
    insert_optional(
        &mut metadata,
        "client_session_dir",
        &request.client_session_dir,
    );
    insert_optional(&mut metadata, "client_cwd", &request.client_cwd);
    metadata.insert("launch_cwd".to_string(), json!(launch_cwd));
    metadata.insert("workspace".to_string(), json!(launch_cwd));
    metadata.insert(
        "runtime_instance_id".to_string(),
        json!(request.runtime_instance_id),
    );
    insert_optional(&mut metadata, "start_command", &request.start_command);
    metadata.insert("runtime_dir".to_string(), json!(runtime_dir));
    metadata.insert("current_turn_file".to_string(), json!(current_turn_file));
    metadata.insert("internal_event_url".to_string(), json!(internal_event_url));
    metadata.insert("capabilities".to_string(), json!(capabilities));

    if let Some(tmux) = &request.tmux {
        if let Some(socket_path) = non_empty(tmux.socket_path.as_deref()) {
            metadata.insert("tmux_socket_path".to_string(), json!(socket_path));
        }
        if let Some(pane_id) = non_empty(tmux.pane_id.as_deref()) {
            metadata.insert("tmux_pane_id".to_string(), json!(pane_id));
        }
        metadata.insert("tmux".to_string(), tmux_metadata(tmux));
    }

    Value::Object(metadata)
}

fn tmux_metadata(tmux: &RuntimeBindingTmuxRequest) -> Value {
    let mut metadata = serde_json::Map::new();
    insert_optional(&mut metadata, "session_id", &tmux.session_id);
    insert_optional(&mut metadata, "session_name", &tmux.session_name);
    insert_optional(&mut metadata, "window_id", &tmux.window_id);
    if let Some(window_index) = tmux.window_index {
        metadata.insert("window_index".to_string(), json!(window_index));
    }
    insert_optional(&mut metadata, "pane_id", &tmux.pane_id);
    if let Some(pane_index) = tmux.pane_index {
        metadata.insert("pane_index".to_string(), json!(pane_index));
    }
    insert_optional(&mut metadata, "pane_current_path", &tmux.pane_current_path);
    Value::Object(metadata)
}

fn agent_binding_metadata(request: &RuntimeBindingUpsertRequest) -> Value {
    let mut metadata = serde_json::Map::new();
    insert_optional(
        &mut metadata,
        "client_session_file",
        &request.client_session_file,
    );
    insert_optional(
        &mut metadata,
        "client_session_dir",
        &request.client_session_dir,
    );
    insert_optional(&mut metadata, "client_cwd", &request.client_cwd);
    Value::Object(metadata)
}

fn insert_optional(
    metadata: &mut serde_json::Map<String, Value>,
    key: &str,
    value: &Option<String>,
) {
    if let Some(value) = non_empty(value.as_deref()) {
        metadata.insert(key.to_string(), json!(value));
    }
}

fn pontia_runtime_dir(session_id: &str) -> Result<PathBuf> {
    if let Ok(path) = std::env::var("PONTIA_DATA_DIR") {
        return Ok(PathBuf::from(path).join("runtimes").join(session_id));
    }
    let home = std::env::var("HOME").map_err(|_| Error::InvalidConfig {
        key: "HOME",
        message: "required to derive pontia data directory".to_string(),
    })?;
    Ok(PathBuf::from(home)
        .join(".local/share/pontia")
        .join("runtimes")
        .join(session_id))
}

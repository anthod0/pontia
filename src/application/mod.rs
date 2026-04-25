use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

use crate::{
    config::AppConfig,
    domain::{
        DomainEvent, EventSource, EventType, ProjectionState, SessionProjection, SessionState,
        TurnProjection, TurnState,
    },
    error::{Error, Result},
    ids::{new_event_id, new_session_id, new_turn_id},
    runtime::{AgentInput, GenericRuntimeManager, RuntimeStartRequest, RuntimeStartResult},
    storage::sqlite::{connect_sqlite, run_migrations},
};

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
    pub external_api_token: Option<String>,
}

pub async fn initialize(config: &AppConfig) -> Result<AppState> {
    let db = connect_sqlite(&config.database_url).await?;

    if config.run_migrations {
        run_migrations(&db).await?;
    }

    Ok(AppState {
        db,
        external_api_token: config.external_api_token.clone(),
    })
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionCapabilities {
    pub accept_task: bool,
    pub interrupt: bool,
    pub stream_output: bool,
    pub heartbeat: bool,
    pub artifact_sources: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionView {
    pub session_id: String,
    pub client_type: String,
    pub state: String,
    pub current_turn_id: Option<String>,
    pub workspace: Option<String>,
    pub capabilities: SessionCapabilities,
    pub created_at: String,
    pub updated_at: String,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnInputView {
    pub summary: Option<String>,
    pub artifact_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnOutputView {
    pub summary: Option<String>,
    pub artifact_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TurnView {
    pub turn_id: String,
    pub session_id: String,
    pub state: String,
    pub input: TurnInputView,
    pub output: TurnOutputView,
    pub failure: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EventView {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub time: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ArtifactView {
    pub artifact_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub size_bytes: Option<i64>,
    pub preview: Option<String>,
    pub created_at: String,
    pub metadata: Value,
}

#[derive(Clone)]
pub struct ExternalQueryService {
    pool: SqlitePool,
}

impl ExternalQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionView>> {
        let rows = sqlx::query(
            r#"SELECT session_id, client_type, state, current_turn_id, workspace_ref,
                      metadata, created_at, updated_at
               FROM sessions ORDER BY created_at, session_id"#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut sessions = rows
            .into_iter()
            .map(row_to_session_view)
            .collect::<Result<Vec<_>>>()?;
        for session in &mut sessions {
            self.enrich_session_view(session).await?;
        }
        Ok(sessions)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionView>> {
        let row = sqlx::query(
            r#"SELECT session_id, client_type, state, current_turn_id, workspace_ref,
                      metadata, created_at, updated_at
               FROM sessions WHERE session_id = ?"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let mut session = row_to_session_view(row)?;
        self.enrich_session_view(&mut session).await?;
        Ok(Some(session))
    }

    pub async fn list_turns(&self, session_id: &str) -> Result<Vec<TurnView>> {
        let rows = sqlx::query(
            r#"SELECT turn_id, session_id, state, input_summary, output_summary,
                      failure_message, metadata, created_at, updated_at
               FROM turns WHERE session_id = ? ORDER BY created_at, turn_id"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        let mut turns = rows
            .into_iter()
            .map(row_to_turn_view)
            .collect::<Result<Vec<_>>>()?;
        for turn in &mut turns {
            self.enrich_turn_view(turn).await?;
        }
        Ok(turns)
    }

    pub async fn get_turn(&self, session_id: &str, turn_id: &str) -> Result<Option<TurnView>> {
        let row = sqlx::query(
            r#"SELECT turn_id, session_id, state, input_summary, output_summary,
                      failure_message, metadata, created_at, updated_at
               FROM turns WHERE session_id = ? AND turn_id = ?"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?;

        let Some(row) = row else {
            return Ok(None);
        };
        let mut turn = row_to_turn_view(row)?;
        self.enrich_turn_view(&mut turn).await?;
        Ok(Some(turn))
    }

    pub async fn list_session_events(&self, session_id: &str) -> Result<Vec<EventView>> {
        let rows = sqlx::query(
            r#"SELECT event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_event_view).collect()
    }

    pub async fn list_turn_events(
        &self,
        session_id: &str,
        turn_id: &str,
    ) -> Result<Vec<EventView>> {
        let rows = sqlx::query(
            r#"SELECT event_id, session_id, turn_id, source, event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND turn_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .bind(turn_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_event_view).collect()
    }

    pub async fn list_artifacts(&self, session_id: &str) -> Result<Vec<ArtifactView>> {
        let rows = sqlx::query(
            r#"SELECT artifact_id, session_id, turn_id, kind, name, size_bytes, metadata, created_at
               FROM artifacts WHERE session_id = ? ORDER BY created_at, artifact_id"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_artifact_view).collect()
    }

    async fn enrich_session_view(&self, session: &mut SessionView) -> Result<()> {
        let row = sqlx::query("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(&session.session_id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(row) = row {
            let metadata: String = row.try_get("metadata")?;
            let metadata: Value = serde_json::from_str(&metadata)?;
            if let Some(capabilities) = metadata.get("capabilities") {
                session.capabilities = serde_json::from_value(capabilities.clone())?;
            }
        }

        Ok(())
    }

    async fn enrich_turn_view(&self, turn: &mut TurnView) -> Result<()> {
        let rows = sqlx::query(
            r#"SELECT event_type, occurred_at, payload
               FROM events WHERE session_id = ? AND turn_id = ? ORDER BY rowid"#,
        )
        .bind(&turn.session_id)
        .bind(&turn.turn_id)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            let event_type: String = row.try_get("event_type")?;
            let occurred_at: String = row.try_get("occurred_at")?;
            let payload: String = row.try_get("payload")?;
            let payload: Value = serde_json::from_str(&payload)?;

            match event_type.as_str() {
                "turn.created" | "turn.queued" | "turn.started" => {
                    if event_type == "turn.started" && turn.started_at.is_none() {
                        turn.started_at = Some(occurred_at.clone());
                    }
                    if turn.input.summary.is_none() {
                        turn.input.summary = nested_string(&payload, &["input", "summary"])
                            .or_else(|| nested_string(&payload, &["input_summary"]));
                    }
                    if turn.input.artifact_id.is_none() {
                        turn.input.artifact_id = nested_string(&payload, &["input", "artifact_id"])
                            .or_else(|| nested_string(&payload, &["input_artifact_id"]));
                    }
                }
                "turn.output" | "turn.completed" => {
                    if event_type == "turn.completed" && turn.state != "completed" {
                        continue;
                    }
                    if event_type == "turn.completed" {
                        turn.completed_at = Some(occurred_at.clone());
                    }
                    if turn.output.summary.is_none() {
                        turn.output.summary = nested_string(&payload, &["output", "summary"])
                            .or_else(|| nested_string(&payload, &["output_summary"]));
                    }
                    if turn.output.artifact_ids.is_empty()
                        && let Some(ids) =
                            nested_array_strings(&payload, &["output", "artifact_ids"])
                                .or_else(|| nested_array_strings(&payload, &["artifact_ids"]))
                    {
                        turn.output.artifact_ids = ids;
                    }
                    if event_type == "turn.completed" {
                        break;
                    }
                }
                "turn.failed" | "turn.interrupted" | "turn.cancelled" => {
                    let expected_state = event_type.strip_prefix("turn.").unwrap_or_default();
                    if turn.state != expected_state {
                        continue;
                    }
                    turn.completed_at = Some(occurred_at);
                    if turn.failure.is_none() {
                        turn.failure = nested_string(&payload, &["failure", "message"])
                            .or_else(|| nested_string(&payload, &["message"]));
                    }
                    break;
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn get_artifact(&self, artifact_id: &str) -> Result<Option<ArtifactView>> {
        let row = sqlx::query(
            r#"SELECT artifact_id, session_id, turn_id, kind, name, size_bytes, metadata, created_at
               FROM artifacts WHERE artifact_id = ?"#,
        )
        .bind(artifact_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_artifact_view).transpose()
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CreateSessionRequest {
    #[serde(default = "default_client_type")]
    pub client_type: String,
    pub workspace: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub initial_task: Option<InitialTaskRequest>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct InitialTaskRequest {
    pub input: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateSessionOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct SessionCommandService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl SessionCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub async fn create_session(
        &self,
        request: CreateSessionRequest,
        idempotency_key: Option<&str>,
    ) -> Result<CreateSessionOutcome> {
        if request.client_type != "generic" {
            return Err(crate::error::Error::Domain(format!(
                "unsupported client_type: {}",
                request.client_type
            )));
        }

        if let Some(key) = idempotency_key
            && let Some(response) = self.idempotency_response("create_session", key).await?
        {
            return Ok(CreateSessionOutcome {
                data: response,
                duplicate: true,
            });
        }

        let session_id = new_session_id().to_string();
        let ingest = EventIngestService::new(self.pool.clone());

        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::ExternalApi,
                request.client_type.clone(),
                EventType::SessionCreated,
                json!({
                    "workspace": request.workspace,
                    "metadata": request.metadata,
                }),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::ExternalApi,
                request.client_type.clone(),
                EventType::SessionStarting,
                json!({}),
            ))
            .await?;

        let runtime = self.runtime.start_session(RuntimeStartRequest {
            session_id: session_id.clone(),
            client_type: request.client_type.clone(),
            workspace: request.workspace.clone(),
        });
        self.upsert_runtime_binding(&session_id, &runtime).await?;
        self.update_session_workspace(&session_id, request.workspace.as_deref())
            .await?;

        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::RuntimeManager,
                request.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.clone(),
                None,
                EventSource::RuntimeManager,
                request.client_type.clone(),
                EventType::SessionReady,
                json!({}),
            ))
            .await?;

        let initial_turn_id = if let Some(initial_task) = request.initial_task {
            let turn_id = new_turn_id().to_string();
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.clone(),
                    Some(turn_id.clone()),
                    EventSource::ExternalApi,
                    request.client_type.clone(),
                    EventType::TurnCreated,
                    json!({
                        "input": { "summary": initial_task.input },
                        "metadata": initial_task.metadata,
                    }),
                ))
                .await?;
            ingest
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.clone(),
                    Some(turn_id.clone()),
                    EventSource::ExternalApi,
                    request.client_type.clone(),
                    EventType::TurnQueued,
                    json!({}),
                ))
                .await?;
            Some(turn_id)
        } else {
            None
        };

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(&session_id)
            .await?
            .ok_or_else(|| crate::error::Error::Domain("created session missing".to_string()))?;
        let initial_turn = if let Some(turn_id) = initial_turn_id {
            query.get_turn(&session_id, &turn_id).await?
        } else {
            None
        };
        let data = json!({ "session": session, "initial_turn": initial_turn });

        if let Some(key) = idempotency_key {
            self.store_idempotency_response("create_session", key, &data)
                .await?;
        }

        Ok(CreateSessionOutcome {
            data,
            duplicate: false,
        })
    }

    async fn idempotency_response(&self, operation: &str, key: &str) -> Result<Option<Value>> {
        let response: Option<String> = sqlx::query_scalar(
            "SELECT response FROM idempotency_keys WHERE operation = ? AND key = ?",
        )
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        response
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (operation, key, response)
               VALUES (?, ?, ?)
               ON CONFLICT(operation, key) DO NOTHING"#,
        )
        .bind(operation)
        .bind(key)
        .bind(serde_json::to_string(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_runtime_binding(
        &self,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_ref = excluded.runtime_ref,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(session_id)
        .bind(&runtime.runtime_kind)
        .bind(&runtime.runtime_ref)
        .bind(serde_json::to_string(&runtime.binding_metadata())?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn update_session_workspace(
        &self,
        session_id: &str,
        workspace: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET workspace_ref = ? WHERE session_id = ?")
            .bind(workspace)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SubmitTurnRequest {
    pub input: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SubmitTurnOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct TurnCommandService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl TurnCommandService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub async fn submit_turn(
        &self,
        session_id: &str,
        request: SubmitTurnRequest,
        idempotency_key: Option<&str>,
    ) -> Result<SubmitTurnOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("submit_turn:{session_id}"), key)
                .await?
        {
            return Ok(SubmitTurnOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        if !matches!(session.state.as_str(), "idle" | "interrupted") {
            return Err(Error::StateConflict(format!(
                "session {session_id} in state {} cannot accept a new turn",
                session.state
            )));
        }

        if let Some(active_turn_id) = &session.current_turn_id {
            return Err(Error::StateConflict(format!(
                "session {session_id} already has active turn {active_turn_id}"
            )));
        }

        if !session.capabilities.accept_task {
            return Err(Error::Domain(format!(
                "session {session_id} runtime cannot accept tasks"
            )));
        }

        let turn_id = new_turn_id().to_string();
        self.runtime.submit_input(AgentInput {
            session_id: session_id.to_string(),
            turn_id: turn_id.clone(),
            input: request.input.clone(),
        })?;

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.clone()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnCreated,
                json!({
                    "input": { "summary": request.input },
                    "metadata": request.metadata,
                }),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.clone()),
                EventSource::ExternalApi,
                session.client_type,
                EventType::TurnQueued,
                json!({}),
            ))
            .await?;

        let mut turn = query
            .get_turn(session_id, &turn_id)
            .await?
            .ok_or_else(|| Error::Domain("submitted turn missing".to_string()))?;
        query.enrich_turn_view(&mut turn).await?;
        let data = json!({ "turn": turn });

        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("submit_turn:{session_id}"), key, &data)
                .await?;
        }

        Ok(SubmitTurnOutcome {
            data,
            duplicate: false,
        })
    }

    async fn idempotency_response(&self, operation: &str, key: &str) -> Result<Option<Value>> {
        let response: Option<String> = sqlx::query_scalar(
            "SELECT response FROM idempotency_keys WHERE operation = ? AND key = ?",
        )
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        response
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (operation, key, response)
               VALUES (?, ?, ?)
               ON CONFLICT(operation, key) DO NOTHING"#,
        )
        .bind(operation)
        .bind(key)
        .bind(serde_json::to_string(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ControlCommandOutcome {
    pub data: Value,
    pub duplicate: bool,
}

#[derive(Clone)]
pub struct RuntimeControlService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl RuntimeControlService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
    }

    pub async fn interrupt_current_turn(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("interrupt_current:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let turn_id = session.current_turn_id.clone().ok_or_else(|| {
            Error::StateConflict(format!(
                "session {session_id} has no active turn to interrupt"
            ))
        })?;
        let outcome = self.interrupt_turn(session_id, &turn_id, None).await?;
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(
                &format!("interrupt_current:{session_id}"),
                key,
                &outcome.data,
            )
            .await?;
        }
        Ok(outcome)
    }

    pub async fn interrupt_turn(
        &self,
        session_id: &str,
        turn_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("interrupt_turn:{session_id}:{turn_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("turn {turn_id} not found")))?;

        if matches!(
            turn.state.as_str(),
            "completed" | "failed" | "interrupted" | "cancelled"
        ) {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is already terminal"
            )));
        }
        if session.current_turn_id.as_deref() != Some(turn_id) {
            return Err(Error::StateConflict(format!(
                "turn {turn_id} is not the active turn for session {session_id}"
            )));
        }
        if !session.capabilities.interrupt {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} runtime does not support interrupt"
            )));
        }

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.to_string()),
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::TurnInterruptRequested,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                Some(turn_id.to_string()),
                EventSource::RuntimeManager,
                session.client_type,
                EventType::TurnInterrupted,
                json!({}),
            ))
            .await?;

        let turn = query
            .get_turn(session_id, turn_id)
            .await?
            .ok_or_else(|| Error::Domain("interrupted turn missing".to_string()))?;
        let data = json!({ "turn": turn });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(
                &format!("interrupt_turn:{session_id}:{turn_id}"),
                key,
                &data,
            )
            .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn terminate_session(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("terminate_session:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;

        if !matches!(session.state.as_str(), "exited" | "error") {
            self.runtime.terminate_session(session_id)?;
            EventIngestService::new(self.pool.clone())
                .ingest_event(DomainEvent::new(
                    new_event_id().to_string(),
                    session_id.to_string(),
                    None,
                    EventSource::RuntimeManager,
                    session.client_type,
                    EventType::SessionExited,
                    json!({}),
                ))
                .await?;
        }

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("terminated session missing".to_string()))?;
        let data = json!({ "session": session });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("terminate_session:{session_id}"), key, &data)
                .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    pub async fn restart_session(
        &self,
        session_id: &str,
        idempotency_key: Option<&str>,
    ) -> Result<ControlCommandOutcome> {
        if let Some(key) = idempotency_key
            && let Some(response) = self
                .idempotency_response(&format!("restart_session:{session_id}"), key)
                .await?
        {
            return Ok(ControlCommandOutcome {
                data: response,
                duplicate: true,
            });
        }

        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if matches!(session.state.as_str(), "exited" | "error") {
            return Err(Error::StateConflict(format!(
                "terminal session {session_id} cannot be restarted"
            )));
        }

        let ingest = EventIngestService::new(self.pool.clone());
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::ExternalApi,
                session.client_type.clone(),
                EventType::SessionStarting,
                json!({}),
            ))
            .await?;
        let runtime = self.runtime.restart_session(RuntimeStartRequest {
            session_id: session_id.to_string(),
            client_type: session.client_type.clone(),
            workspace: session.workspace.clone(),
        });
        self.upsert_runtime_binding(session_id, &runtime).await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::RuntimeManager,
                session.client_type.clone(),
                EventType::SessionStarted,
                json!({}),
            ))
            .await?;
        ingest
            .ingest_event(DomainEvent::new(
                new_event_id().to_string(),
                session_id.to_string(),
                None,
                EventSource::RuntimeManager,
                session.client_type,
                EventType::SessionReady,
                json!({}),
            ))
            .await?;

        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::Domain("restarted session missing".to_string()))?;
        let data = json!({ "session": session });
        if let Some(key) = idempotency_key {
            self.store_idempotency_response(&format!("restart_session:{session_id}"), key, &data)
                .await?;
        }
        Ok(ControlCommandOutcome {
            data,
            duplicate: false,
        })
    }

    async fn idempotency_response(&self, operation: &str, key: &str) -> Result<Option<Value>> {
        let response: Option<String> = sqlx::query_scalar(
            "SELECT response FROM idempotency_keys WHERE operation = ? AND key = ?",
        )
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        response
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (operation, key, response)
               VALUES (?, ?, ?)
               ON CONFLICT(operation, key) DO NOTHING"#,
        )
        .bind(operation)
        .bind(key)
        .bind(serde_json::to_string(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_runtime_binding(
        &self,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_ref = excluded.runtime_ref,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(session_id)
        .bind(&runtime.runtime_kind)
        .bind(&runtime.runtime_ref)
        .bind(serde_json::to_string(&runtime.binding_metadata())?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn default_client_type() -> String {
    "generic".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventIngestResult {
    pub accepted: bool,
    pub duplicate: bool,
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub state_version: i64,
}

#[derive(Clone)]
pub struct EventIngestService {
    pool: SqlitePool,
}

impl EventIngestService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn ingest_event(&self, event: DomainEvent) -> Result<EventIngestResult> {
        if let Some(existing_version) = self
            .existing_event_state_version(&event.event_id, &event.session_id)
            .await?
        {
            return Ok(EventIngestResult {
                accepted: true,
                duplicate: true,
                event_id: event.event_id,
                session_id: event.session_id,
                turn_id: event.turn_id,
                state_version: existing_version,
            });
        }

        let sessions = self.load_session_projection(&event.session_id).await?;
        let turns = self.load_turn_projections(&event.session_id).await?;
        let mut projection = ProjectionState::with_existing(sessions, turns);
        projection.apply(&event)?;

        let mut tx = self.pool.begin().await?;
        let payload = serde_json::to_string(&event.payload)?;
        let occurred_at = event
            .occurred_at
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|err| {
                crate::error::Error::Domain(format!("invalid event timestamp: {err}"))
            })?;

        sqlx::query(
            r#"INSERT INTO events
               (event_id, session_id, turn_id, source, client_type, event_type, occurred_at, seq, payload)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&event.event_id)
        .bind(&event.session_id)
        .bind(&event.turn_id)
        .bind(event.source.to_string())
        .bind(&event.client_type)
        .bind(event.event_type.to_string())
        .bind(occurred_at)
        .bind(event.seq)
        .bind(payload)
        .execute(&mut *tx)
        .await?;

        let state_version: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
                .bind(&event.session_id)
                .fetch_one(&mut *tx)
                .await?;

        for session in projection.sessions() {
            let metadata = serde_json::to_string(&session.metadata)?;
            sqlx::query(
                r#"INSERT INTO sessions
                   (session_id, client_type, state, current_turn_id, state_version, metadata)
                   VALUES (?, ?, ?, ?, ?, ?)
                   ON CONFLICT(session_id) DO UPDATE SET
                       client_type = excluded.client_type,
                       state = excluded.state,
                       current_turn_id = excluded.current_turn_id,
                       state_version = excluded.state_version,
                       metadata = excluded.metadata,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
            )
            .bind(&session.session_id)
            .bind(&session.client_type)
            .bind(session.state.to_string())
            .bind(&session.current_turn_id)
            .bind(state_version)
            .bind(metadata)
            .execute(&mut *tx)
            .await?;
        }

        for turn in projection.turns() {
            let metadata = serde_json::to_string(&turn.metadata)?;
            sqlx::query(
                r#"INSERT INTO turns
                   (turn_id, session_id, state, state_version, metadata)
                   VALUES (?, ?, ?, ?, ?)
                   ON CONFLICT(turn_id) DO UPDATE SET
                       session_id = excluded.session_id,
                       state = excluded.state,
                       state_version = excluded.state_version,
                       metadata = excluded.metadata,
                       updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
            )
            .bind(&turn.turn_id)
            .bind(&turn.session_id)
            .bind(turn.state.to_string())
            .bind(turn.state_version)
            .bind(metadata)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;

        Ok(EventIngestResult {
            accepted: true,
            duplicate: false,
            event_id: event.event_id,
            session_id: event.session_id,
            turn_id: event.turn_id,
            state_version,
        })
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionProjection>> {
        let mut sessions = self.load_session_projection(session_id).await?;
        Ok(sessions.pop())
    }

    pub async fn get_turn(&self, turn_id: &str) -> Result<Option<TurnProjection>> {
        let row = sqlx::query(
            "SELECT turn_id, session_id, state, state_version, metadata FROM turns WHERE turn_id = ?",
        )
        .bind(turn_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(row_to_turn).transpose()
    }

    pub async fn list_events(&self, session_id: &str) -> Result<Vec<DomainEvent>> {
        let rows = sqlx::query(
            r#"SELECT event_id, session_id, turn_id, source, client_type, event_type, occurred_at, seq, payload
               FROM events WHERE session_id = ? ORDER BY rowid"#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_event).collect()
    }

    pub async fn sequence_warnings(&self, event: &DomainEvent) -> Result<Vec<String>> {
        let Some(seq) = event.seq else {
            return Ok(Vec::new());
        };

        let max_seq: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(seq) FROM events WHERE session_id = ? AND seq IS NOT NULL",
        )
        .bind(&event.session_id)
        .fetch_one(&self.pool)
        .await?;

        let Some(max_seq) = max_seq else {
            return Ok(Vec::new());
        };

        let warning = if seq <= max_seq {
            Some(format!(
                "non-monotonic sequence: received seq {seq} after max seq {max_seq}"
            ))
        } else if seq > max_seq + 1 {
            Some(format!(
                "sequence gap: received seq {seq} after max seq {max_seq}"
            ))
        } else {
            None
        };

        Ok(warning.into_iter().collect())
    }

    pub async fn record_warnings(&self, event: &DomainEvent, warnings: &[String]) -> Result<()> {
        for warning in warnings {
            sqlx::query(
                "INSERT INTO ingest_warnings (event_id, session_id, warning) VALUES (?, ?, ?)",
            )
            .bind(&event.event_id)
            .bind(&event.session_id)
            .bind(warning)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn existing_event_state_version(
        &self,
        event_id: &str,
        session_id: &str,
    ) -> Result<Option<i64>> {
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM events WHERE event_id = ?")
            .bind(event_id)
            .fetch_optional(&self.pool)
            .await?;

        if exists.is_none() {
            return Ok(None);
        }

        let version = sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&self.pool)
            .await?;
        Ok(Some(version))
    }

    async fn load_session_projection(&self, session_id: &str) -> Result<Vec<SessionProjection>> {
        let rows = sqlx::query(
            "SELECT session_id, client_type, state, current_turn_id, state_version, metadata FROM sessions WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_session).collect()
    }

    async fn load_turn_projections(&self, session_id: &str) -> Result<Vec<TurnProjection>> {
        let rows = sqlx::query(
            "SELECT turn_id, session_id, state, state_version, metadata FROM turns WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_turn).collect()
    }
}

fn nested_string(value: &Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str().map(ToString::to_string)
}

fn nested_array_strings(value: &Value, path: &[&str]) -> Option<Vec<String>> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(
        current
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(ToString::to_string)
            .collect(),
    )
}

fn remove_internal_metadata_fields(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("source_ref");
    }
}

fn row_to_session_view(row: sqlx::sqlite::SqliteRow) -> Result<SessionView> {
    let metadata: String = row.try_get("metadata")?;

    Ok(SessionView {
        session_id: row.try_get("session_id")?,
        client_type: row.try_get("client_type")?,
        state: row.try_get("state")?,
        current_turn_id: row.try_get("current_turn_id")?,
        workspace: row.try_get("workspace_ref")?,
        capabilities: SessionCapabilities::default(),
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        metadata: serde_json::from_str(&metadata)?,
    })
}

fn row_to_turn_view(row: sqlx::sqlite::SqliteRow) -> Result<TurnView> {
    let metadata: String = row.try_get("metadata")?;
    let metadata_json: Value = serde_json::from_str(&metadata)?;
    let artifact_ids = metadata_json
        .get("artifact_ids")
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default();

    Ok(TurnView {
        turn_id: row.try_get("turn_id")?,
        session_id: row.try_get("session_id")?,
        state: row.try_get("state")?,
        input: TurnInputView {
            summary: row.try_get("input_summary")?,
            artifact_id: metadata_json
                .get("input_artifact_id")
                .and_then(Value::as_str)
                .map(ToString::to_string),
        },
        output: TurnOutputView {
            summary: row.try_get("output_summary")?,
            artifact_ids,
        },
        failure: row.try_get("failure_message")?,
        created_at: row.try_get("created_at")?,
        started_at: None,
        completed_at: None,
        metadata: metadata_json,
    })
}

fn row_to_event_view(row: sqlx::sqlite::SqliteRow) -> Result<EventView> {
    let payload: String = row.try_get("payload")?;

    Ok(EventView {
        event_id: row.try_get("event_id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        source: row.try_get("source")?,
        event_type: row.try_get("event_type")?,
        time: row.try_get("occurred_at")?,
        payload: serde_json::from_str(&payload)?,
    })
}

fn row_to_artifact_view(row: sqlx::sqlite::SqliteRow) -> Result<ArtifactView> {
    let metadata: String = row.try_get("metadata")?;
    let mut metadata_json: Value = serde_json::from_str(&metadata)?;
    remove_internal_metadata_fields(&mut metadata_json);

    Ok(ArtifactView {
        artifact_id: row.try_get("artifact_id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        kind: row.try_get("kind")?,
        name: row.try_get("name")?,
        size_bytes: row.try_get("size_bytes")?,
        preview: metadata_json
            .get("preview")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        created_at: row.try_get("created_at")?,
        metadata: metadata_json,
    })
}

fn row_to_session(row: sqlx::sqlite::SqliteRow) -> Result<SessionProjection> {
    let metadata: String = row.try_get("metadata")?;
    let state: String = row.try_get("state")?;

    Ok(SessionProjection {
        session_id: row.try_get("session_id")?,
        client_type: row.try_get("client_type")?,
        state: SessionState::from_str(&state)?,
        current_turn_id: row.try_get("current_turn_id")?,
        state_version: row.try_get("state_version")?,
        metadata: serde_json::from_str(&metadata)?,
    })
}

fn row_to_turn(row: sqlx::sqlite::SqliteRow) -> Result<TurnProjection> {
    let metadata: String = row.try_get("metadata")?;
    let state: String = row.try_get("state")?;

    Ok(TurnProjection {
        turn_id: row.try_get("turn_id")?,
        session_id: row.try_get("session_id")?,
        state: TurnState::from_str(&state)?,
        state_version: row.try_get("state_version")?,
        metadata: serde_json::from_str(&metadata)?,
    })
}

fn row_to_event(row: sqlx::sqlite::SqliteRow) -> Result<DomainEvent> {
    let payload: String = row.try_get("payload")?;
    let source: String = row.try_get("source")?;
    let event_type: String = row.try_get("event_type")?;
    let occurred_at: String = row.try_get("occurred_at")?;

    Ok(DomainEvent {
        event_id: row.try_get("event_id")?,
        session_id: row.try_get("session_id")?,
        turn_id: row.try_get("turn_id")?,
        source: EventSource::from_str(&source)?,
        client_type: row.try_get("client_type")?,
        event_type: EventType::from_str(&event_type)?,
        occurred_at: time::OffsetDateTime::parse(
            &occurred_at,
            &time::format_description::well_known::Rfc3339,
        )
        .map_err(|err| crate::error::Error::Domain(format!("invalid event timestamp: {err}")))?,
        seq: row.try_get("seq")?,
        payload: serde_json::from_str(&payload)?,
    })
}

mod events;
mod observer;

use crate::{
    AgentBindingService, EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType,
    ReportedFact, UpsertAgentBindingRequest,
};
use pontia_core::{Error, Result, ids::new_turn_id};
use pontia_runtime::{RuntimeStartResult, codex::CodexRuntime};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub use observer::CodexObserver;

#[derive(Clone)]
pub struct CodexService {
    pub(super) pool: SqlitePool,
}

impl CodexService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn provision(&self, session_id: &str, root: &Path, cwd: &Path) -> Result<()> {
        let runtime = RuntimeStartResult {
            runtime_kind: "codex_app_server".into(),
            runtime_handle: root.display().to_string(),
            capabilities: pontia_agent_clients::codex::CAPABILITIES,
            metadata: json!({"launch_cwd":cwd,"codex":{"connection":"awaiting_input"}}),
        };
        pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository::new(
            self.pool.clone(),
        )
        .upsert_binding(crate::runtime_control::runtime_binding_record(
            session_id, &runtime,
        )?)
        .await
    }

    pub(super) async fn root(&self, session_id: &str) -> Result<PathBuf> {
        let root: String = sqlx::query_scalar("SELECT runtime_handle FROM runtime_bindings WHERE session_id=? AND runtime_kind='codex_app_server'")
            .bind(session_id).fetch_one(&self.pool).await?;
        Ok(root.into())
    }

    pub(super) async fn runtime(&self, session_id: &str) -> Result<Arc<CodexRuntime>> {
        CodexRuntime::ensure(&self.root(session_id).await?).await
    }

    pub(super) async fn bind(
        &self,
        session_id: &str,
        runtime: &CodexRuntime,
        thread: &Value,
    ) -> Result<()> {
        let id = string(thread, "id")?;
        let cwd = thread["cwd"]
            .as_str()
            .ok_or_else(|| Error::Domain("Codex thread has no cwd".into()))?;
        AgentBindingService::new(self.pool.clone())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session_id.into(),
                client_type: "codex".into(),
                launch_cwd: cwd.into(),
                client_session_key: id.into(),
                client_session_file: thread["path"].as_str().map(str::to_string),
                metadata: json!({}),
            })
            .await?;
        let mut capabilities = pontia_agent_clients::codex::CAPABILITIES;
        capabilities.timeline = thread["path"].as_str().is_some_and(|path| {
            pontia_agent_clients::codex::rollout::identity(Path::new(path))
                .is_ok_and(|native| native == id)
        });
        let capabilities = serde_json::to_string(&capabilities)?;
        sqlx::query("UPDATE runtime_bindings SET runtime_instance_id=?, binding_state='confirmed', capabilities=?, adapter_details=json_set(adapter_details,'$.codex',json(?)) WHERE session_id=?")
            .bind(&runtime.instance_id).bind(capabilities)
            .bind(json!({"thread_id":id,"endpoint":format!("unix://{}",runtime.socket_path.display()),"connection":"reconciling"}).to_string())
            .bind(session_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn submit(
        &self,
        session_id: &str,
        input: &str,
        message_id: Option<&str>,
    ) -> Result<()> {
        let runtime = self.runtime(session_id).await?;
        let _operation = runtime.lock_session(session_id).await;
        let connection = runtime.connection().await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?;
        let new_thread = binding.is_none();
        let thread = match binding {
            Some(binding) => {
                let result = connection
                    .call(
                        "thread/resume",
                        json!({"threadId":binding.client_session_key,"excludeTurns":true}),
                    )
                    .await?;
                result["thread"].clone()
            }
            None => {
                let cwd: String = sqlx::query_scalar(
                    "SELECT launch_cwd FROM runtime_bindings WHERE session_id=?",
                )
                .bind(session_id)
                .fetch_one(&self.pool)
                .await?;
                let environment: Option<String> = sqlx::query_scalar("SELECT json_extract(adapter_details,'$.codex_environment') FROM runtime_bindings WHERE session_id=?").bind(session_id).fetch_one(&self.pool).await?;
                let environment: Value = environment
                    .map(|value| serde_json::from_str(&value))
                    .transpose()?
                    .unwrap_or_else(|| json!({}));
                let result = connection
                    .call(
                        "thread/start",
                        json!({"cwd":cwd,"config":{"shell_environment_policy.set":environment}}),
                    )
                    .await?;
                result["thread"].clone()
            }
        };
        self.bind(session_id, &runtime, &thread).await?;
        let thread_id = string(&thread, "id")?;
        // thread/start subscribes this connection. A resumed thread needs its actual turns,
        // because excludeTurns intentionally returned no execution history.
        let turns = if new_thread {
            Vec::new()
        } else {
            self.turns(&connection, thread_id).await?
        };
        self.reconcile_turns(session_id, &runtime, &turns).await?;
        self.ready(session_id, &runtime, &thread).await?;
        self.connection_state(session_id, &runtime.instance_id, "available")
            .await?;
        let active = turns.iter().find(|turn| turn["status"] == "inProgress");
        let mut params = json!({"threadId":thread_id,"input":[{"type":"text","text":input}]});
        if let Some(message_id) = message_id {
            params["clientUserMessageId"] = json!(message_id);
        }
        if let Some(turn) = active {
            params["expectedTurnId"] = turn["id"].clone();
        }
        let result = connection
            .call(
                if active.is_some() {
                    "turn/steer"
                } else {
                    "turn/start"
                },
                params,
            )
            .await;
        match result {
            Ok(accepted) => {
                // Acceptance only acknowledges delivery. Status comes from notifications/snapshots.
                if let Some(message_id) = message_id {
                    let native_turn = accepted
                        .get("turnId")
                        .or_else(|| accepted.pointer("/turn/id"));
                    if let Some(native_turn) = native_turn.and_then(Value::as_str) {
                        sqlx::query("UPDATE inbox_messages SET metadata=json_set(CASE WHEN json_type(metadata)='object' THEN metadata ELSE '{}' END,'$.codex_turn_id',?) WHERE message_id=? AND session_id=?")
                            .bind(native_turn).bind(message_id).bind(session_id).execute(&self.pool).await?;
                    }
                }
                if let Err(error) = self
                    .open_tui_with_runtime(session_id, &runtime, &thread)
                    .await
                {
                    tracing::warn!(%session_id, %error, "Codex input delivered but TUI could not attach");
                }
                Ok(())
            }
            Err(error) => {
                if !connection.is_connected() {
                    self.connection_state(session_id, &runtime.instance_id, "unavailable")
                        .await?;
                }
                Err(error)
            }
        }
    }

    pub async fn interrupt(&self, session_id: &str) -> Result<()> {
        let runtime = self.runtime(session_id).await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
            .ok_or_else(|| Error::StateConflict("Codex has not received its first input".into()))?;
        let connection = runtime.connection().await?;
        let result = connection
            .call(
                "thread/resume",
                json!({"threadId":binding.client_session_key,"excludeTurns":true}),
            )
            .await?;
        self.bind(session_id, &runtime, &result["thread"]).await?;
        let turns = self.turns(&connection, &binding.client_session_key).await?;
        let active = turns
            .iter()
            .find(|turn| turn["status"] == "inProgress")
            .ok_or_else(|| Error::StateConflict("Codex has no active turn".into()))?;
        connection
            .call(
                "turn/interrupt",
                json!({"threadId":binding.client_session_key,"turnId":active["id"]}),
            )
            .await?;
        Ok(())
    }

    pub async fn archive(&self, session_id: &str) -> Result<()> {
        let runtime = self.runtime(session_id).await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
            .ok_or_else(|| Error::StateConflict("Codex has not created a thread yet".into()))?;
        runtime
            .connection()
            .await?
            .call(
                "thread/archive",
                json!({"threadId":binding.client_session_key}),
            )
            .await?;
        // Only the archived notification or a verified archived listing confirms exit.
        self.check_archived(session_id, &runtime, &binding.client_session_key)
            .await?;
        Ok(())
    }

    pub async fn resume(&self, session_id: &str) -> Result<()> {
        let runtime = self.runtime(session_id).await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
            .ok_or_else(|| Error::StateConflict("Codex thread binding is missing".into()))?;
        let connection = runtime.connection().await?;
        connection
            .call(
                "thread/unarchive",
                json!({"threadId":binding.client_session_key}),
            )
            .await?;
        let result = connection
            .call(
                "thread/resume",
                json!({"threadId":binding.client_session_key,"excludeTurns":true}),
            )
            .await?;
        self.bind(session_id, &runtime, &result["thread"]).await?;
        let turns = self.turns(&connection, &binding.client_session_key).await?;
        self.reconcile_turns(session_id, &runtime, &turns).await?;
        self.ready(session_id, &runtime, &result["thread"]).await?;
        self.connection_state(session_id, &runtime.instance_id, "available")
            .await?;
        self.open_tui_with_runtime(session_id, &runtime, &result["thread"])
            .await
    }

    pub async fn open_tui(&self, session_id: &str) -> Result<()> {
        let runtime = self.runtime(session_id).await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
            .ok_or_else(|| {
                Error::StateConflict("Send the first message before opening Codex TUI".into())
            })?;
        self.open_tui_with_runtime(
            session_id,
            &runtime,
            &json!({"id":binding.client_session_key,"cwd":binding.launch_cwd}),
        )
        .await
    }

    async fn open_tui_with_runtime(
        &self,
        session_id: &str,
        runtime: &Arc<CodexRuntime>,
        thread: &Value,
    ) -> Result<()> {
        let owner: Option<String> = sqlx::query_scalar("SELECT owner_session_id FROM codex_tui_bindings WHERE target_session_id=? AND runtime_instance_id=? AND connected=TRUE LIMIT 1")
            .bind(session_id).bind(&runtime.instance_id).fetch_optional(&self.pool).await?;
        if owner.is_some() {
            return Ok(());
        }
        // The original owner may now be displaying another thread. Never silently
        // reuse that terminal as though it were still attached to this Session.
        if runtime
            .tui_targets
            .lock()
            .await
            .get(session_id)
            .is_some_and(|target| target.connected && target.thread["id"] != thread["id"])
        {
            return Err(Error::StateConflict("This TUI is displaying another Codex thread; use /resume in that TUI to switch back".into()));
        }
        let (socket, pane) = runtime
            .open_tui(
                session_id,
                string(thread, "id")?,
                Path::new(string(thread, "cwd")?),
            )
            .await?;
        sqlx::query("INSERT INTO codex_tui_bindings(owner_session_id,target_session_id,runtime_instance_id,tmux_socket_path,tmux_pane_id) VALUES(?,?,?,?,?) ON CONFLICT(owner_session_id) DO UPDATE SET runtime_instance_id=excluded.runtime_instance_id,tmux_socket_path=excluded.tmux_socket_path,tmux_pane_id=excluded.tmux_pane_id")
            .bind(session_id).bind(session_id).bind(&runtime.instance_id).bind(socket).bind(pane).execute(&self.pool).await?;
        Ok(())
    }

    pub(super) async fn connection_state(
        &self,
        session_id: &str,
        instance: &str,
        state: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE runtime_bindings SET adapter_details=json_set(adapter_details,'$.codex.connection',?) WHERE session_id=? AND runtime_instance_id=?")
            .bind(state).bind(session_id).bind(instance).execute(&self.pool).await?;
        Ok(())
    }

    pub(super) async fn owned_event(&self, session: &str, kind: PontiaEventType) -> Result<()> {
        EventIngestService::new(self.pool.clone())
            .ingest_pontia_event(PontiaEvent::new(
                session,
                None,
                PontiaEventSource::RuntimeManager,
                "codex",
                kind,
                json!({}),
            ))
            .await?;
        Ok(())
    }
}

pub(super) fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Domain(format!("Codex response missing {field}")))
}

pub(crate) async fn native_turn_identity(pool: &SqlitePool, fact: &ReportedFact) -> Result<String> {
    let native = string(&fact.data, "native_turn_id")?;
    let runtime = string(&fact.data, "runtime_instance_id")?;
    let current: Option<String> =
        sqlx::query_scalar("SELECT runtime_instance_id FROM runtime_bindings WHERE session_id=?")
            .bind(&fact.session_id)
            .fetch_optional(pool)
            .await?
            .flatten();
    if current.as_deref() != Some(runtime) {
        return Err(Error::StateConflict(
            "Codex fact belongs to an obsolete runtime".into(),
        ));
    }
    sqlx::query("INSERT INTO native_turn_bindings(session_id,client_turn_id,turn_id) VALUES(?,?,?) ON CONFLICT(session_id,client_turn_id) DO NOTHING")
        .bind(&fact.session_id).bind(native).bind(new_turn_id().to_string()).execute(pool).await?;
    Ok(sqlx::query_scalar(
        "SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?",
    )
    .bind(&fact.session_id)
    .bind(native)
    .fetch_one(pool)
    .await?)
}

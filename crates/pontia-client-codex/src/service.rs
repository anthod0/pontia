mod adoption;
mod background;
mod fork;
pub(crate) use background::CodexBackgroundService;
mod compaction;
#[cfg(test)]
mod compaction_tests;
pub(crate) mod environment;
#[cfg(test)]
mod environment_tests;
mod events;
mod exit;
#[cfg(test)]
mod interaction_tests;
pub(crate) mod interactions;
mod live_output;
#[cfg(test)]
mod live_output_tests;
mod models;
mod observer;
pub(crate) mod profile;
mod proxy_observer;
#[cfg(test)]
mod subscription_tests;
use crate::runtime::{CodexRuntime, SubscriptionState};
use pontia_application::{AgentBindingService, UpsertAgentBindingRequest};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
use tokio::sync::{Mutex, broadcast};

fn confirmed_exits() -> &'static Mutex<HashSet<String>> {
    static SESSIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SESSIONS.get_or_init(Default::default)
}

fn observation_changes() -> &'static broadcast::Sender<(PathBuf, String)> {
    static CHANGES: OnceLock<broadcast::Sender<(PathBuf, String)>> = OnceLock::new();
    CHANGES.get_or_init(|| broadcast::channel(1024).0)
}

pub use observer::CodexObserver;

#[derive(Clone)]
pub struct CodexService {
    pub(super) pool: SqlitePool,
    pub(super) event_ingest: pontia_application::EventIngestService,
}

impl CodexService {
    pub fn new(event_ingest: pontia_application::EventIngestService) -> Self {
        Self {
            pool: event_ingest.db(),
            event_ingest,
        }
    }

    pub async fn provision(
        &self,
        session_id: &str,
        root: &Path,
        cwd: &Path,
        environment: &std::collections::BTreeMap<String, String>,
    ) -> Result<()> {
        let root = root.canonicalize()?;
        sqlx::query("UPDATE sessions SET metadata=json_set(CASE WHEN json_type(metadata)='object' THEN metadata ELSE '{}' END,'$.codex_control_root',?,'$.codex_environment',json(?),'$.codex_launch_cwd',?) WHERE session_id=?")
            .bind(root.display().to_string())
            .bind(serde_json::to_string(environment)?)
            .bind(cwd.display().to_string())
            .bind(session_id)
            .execute(&self.pool).await?;
        let _ = observation_changes().send((root, session_id.into()));
        Ok(())
    }

    pub(super) async fn root(&self, session_id: &str) -> Result<PathBuf> {
        let root: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(metadata,'$.codex_control_root') FROM sessions WHERE session_id=?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten();
        root.map(Into::into)
            .ok_or_else(|| Error::StateConflict("Codex control root is missing".into()))
    }

    pub(super) async fn runtime(&self, session_id: &str) -> Result<Arc<CodexRuntime>> {
        let runtime = CodexRuntime::ensure(&self.root(session_id).await?).await?;
        runtime.profile_service.get_or_init(|| self.profiles());
        Ok(runtime)
    }

    pub(super) async fn bind(
        &self,
        session_id: &str,
        runtime: &CodexRuntime,
        thread: &Value,
    ) -> Result<()> {
        let _current = runtime.current_guard().await?;
        let id = string(thread, "id")?;
        let cwd = string(thread, "cwd")?;
        AgentBindingService::new(self.pool.clone())
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session_id.into(),
                client_type: "codex".into(),
                launch_cwd: cwd.into(),
                client_session_key: id.into(),
                client_session_file: thread["path"].as_str().map(str::to_owned),
                metadata: json!({}),
            })
            .await?;
        let _ = observation_changes().send((runtime.root.clone(), session_id.into()));
        Ok(())
    }

    pub(crate) async fn submit(
        &self,
        target: &pontia_application::runtime::ControlTarget,
        input: &str,
        message_id: Option<&str>,
        intent: &pontia_application::turns::InputIntent,
    ) -> Result<pontia_application::control::InputReceipt> {
        let session_id = &target.session_id;
        let runtime = self.runtime(session_id).await?;
        let _operation = runtime.lock_session(session_id).await;
        target.validate(&self.pool).await?;
        let connection = runtime.connection().await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?;
        let profile = self.profiles().codex_binding(session_id).await?;
        let (thread, turns) = match binding {
            Some(binding) => {
                if connection
                    .unloaded_threads
                    .lock()
                    .await
                    .contains_key(&binding.client_session_key)
                    || self.reload_suspended(session_id).await?
                {
                    return Err(Error::CapabilityUnavailable(
                        "Codex observation is suspended after unload or an exit attempt; complete Session exit before resuming".into(),
                    ));
                }
                if !matches!(
                    runtime.subscription(session_id).await,
                    Some(SubscriptionState::Available | SubscriptionState::AwaitingFirstInput)
                ) {
                    self.subscribe(session_id, &runtime, &binding.client_session_key, false)
                        .await?;
                }
                let snapshot = connection
                    .call(
                        "thread/read",
                        json!({"threadId":binding.client_session_key,"includeTurns":false}),
                    )
                    .await?;
                let turns = self
                    .turns(&connection, &binding.client_session_key)
                    .await?
                    .unwrap_or_default();
                (snapshot["thread"].clone(), turns)
            }
            None => {
                runtime
                    .set_subscription(session_id, SubscriptionState::Reconciling)
                    .await;
                let row: (Option<String>, Option<String>) = sqlx::query_as("SELECT workspace_ref,json_extract(metadata,'$.codex_launch_cwd') FROM sessions WHERE session_id=?")
                    .bind(session_id).fetch_one(&self.pool).await?;
                let cwd = row
                    .0
                    .or(row.1)
                    .ok_or_else(|| Error::StateConflict("Codex launch cwd is missing".into()))?;
                let config = self.session_config(session_id, &connection, &cwd).await?;
                let mut params = json!({"cwd":cwd,"historyMode":"legacy","config":config});
                profile::apply_profile(&mut params, profile.as_ref())?;
                let response = connection.call("thread/start", params).await?;
                runtime.current_guard().await?;
                let thread = response["thread"].clone();
                self.bind(session_id, &runtime, &thread).await?;
                if profile.is_some() {
                    self.profiles()
                        .confirm_codex_configuration(session_id, string(&thread, "id")?)
                        .await?;
                }
                self.model_snapshot(session_id, &runtime, &response).await?;
                // A newly created thread has no turns and cannot list history before first input.
                self.ready(session_id, &runtime, &thread).await?;
                runtime
                    .set_subscription(session_id, SubscriptionState::Available)
                    .await;
                self.event_ingest.control_available(session_id);
                (thread, Vec::new())
            }
        };
        self.reconcile_turns(session_id, &runtime, &turns).await?;
        let thread_id = string(&thread, "id")?;
        let active = turns.iter().find(|turn| turn["status"] == "inProgress");
        let mut params = json!({"threadId":thread_id,"input":[{"type":"text","text":input}]});
        if let Some(message_id) = message_id {
            params["clientUserMessageId"] = json!(message_id);
        }
        let method = match intent {
            pontia_application::turns::InputIntent::Start if active.is_none() => "turn/start",
            pontia_application::turns::InputIntent::Steer { turn_id } => {
                let native: Option<String> = sqlx::query_scalar("SELECT client_turn_id FROM native_turn_bindings WHERE session_id=? AND turn_id=?")
                    .bind(session_id).bind(turn_id).fetch_optional(&self.pool).await?;
                if native.is_none()
                    || active.and_then(|turn| turn["id"].as_str()) != native.as_deref()
                {
                    return Err(Error::StateConflict(
                        "Codex active turn changed before steer".into(),
                    ));
                }
                params["expectedTurnId"] = json!(native);
                "turn/steer"
            }
            _ => {
                return Err(Error::Conflict {
                    code: "input_busy",
                    message: "Codex is busy; start input was not submitted".into(),
                });
            }
        };
        let result = connection.call(method, params).await;
        runtime.current_guard().await?;
        let accepted = result?;
        // turn/start subscribes its connection even when the thread was only loaded in memory.
        if runtime.subscription(session_id).await == Some(SubscriptionState::AwaitingFirstInput) {
            runtime
                .set_subscription(session_id, SubscriptionState::Available)
                .await;
        }
        let _ = observation_changes().send((runtime.root.clone(), session_id.into()));
        Ok(pontia_application::control::InputReceipt {
            native_turn_id: accepted
                .get("turnId")
                .or_else(|| accepted.pointer("/turn/id"))
                .and_then(Value::as_str)
                .map(str::to_owned),
            runtime_id: None,
        })
    }

    pub(crate) async fn interrupt(
        &self,
        target: &pontia_application::runtime::ControlTarget,
        turn_id: Option<&str>,
    ) -> Result<()> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        self.confirm_control_connection(target, &runtime).await?;
        let binding = self.binding(&target.session_id).await?;
        let expected_native: Option<String> = if let Some(turn_id) = turn_id {
            Some(sqlx::query_scalar(
                "SELECT client_turn_id FROM native_turn_bindings WHERE session_id=? AND turn_id=?",
            ).bind(&target.session_id).bind(turn_id).fetch_one(&self.pool).await?)
        } else {
            None
        };
        let connection = runtime.connection().await?;
        let turns = self
            .turns(&connection, &binding.client_session_key)
            .await?
            .unwrap_or_default();
        let native = turns
            .iter()
            .find(|turn| {
                turn["status"] == "inProgress"
                    && expected_native
                        .as_ref()
                        .is_none_or(|expected| turn["id"] == *expected)
            })
            .and_then(|turn| turn["id"].as_str())
            .ok_or_else(|| {
                Error::StateConflict("Codex active turn changed before interrupt".into())
            })?;
        connection
            .call(
                "turn/interrupt",
                json!({"threadId":binding.client_session_key,"turnId":native}),
            )
            .await?;
        runtime.current_guard().await?;
        Ok(())
    }

    pub(crate) async fn resume(
        &self,
        target: &pontia_application::runtime::ControlTarget,
    ) -> Result<()> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        target.validate(&self.pool).await?;
        let binding = self.binding(&target.session_id).await?;
        confirmed_exits().lock().await.remove(&target.session_id);
        self.subscribe(
            &target.session_id,
            &runtime,
            &binding.client_session_key,
            true,
        )
        .await?;
        sqlx::query("UPDATE sessions SET metadata=json_remove(metadata,'$.codex_observation_unloaded') WHERE session_id=?")
            .bind(&target.session_id).execute(&self.pool).await?;
        Ok(())
    }

    pub(super) async fn has_confirmed_exit(session: &str) -> bool {
        confirmed_exits().lock().await.contains(session)
    }

    async fn reload_suspended(&self, session: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, bool>("SELECT COALESCE(json_extract(metadata,'$.codex_observation_unloaded'),false) FROM sessions WHERE session_id=?")
            .bind(session).fetch_one(&self.pool).await?)
    }

    async fn suspend_reload(&self, session: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET metadata=json_set(metadata,'$.codex_observation_unloaded',json('true')) WHERE session_id=?")
            .bind(session).execute(&self.pool).await?;
        Ok(())
    }

    pub(super) async fn subscribe(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        thread_id: &str,
        reload: bool,
    ) -> Result<()> {
        let connection = runtime.connection().await?;
        let params = self.resume_params(session, thread_id, &connection).await?;
        self.subscribe_with_params(session, runtime, thread_id, reload, params)
            .await
    }

    async fn subscribe_with_params(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        thread_id: &str,
        reload: bool,
        params: Value,
    ) -> Result<()> {
        runtime
            .set_subscription(session, SubscriptionState::Reconciling)
            .await;
        let connection = runtime.connection().await?;
        let resumed = connection.resume_thread(params, reload).await;
        let (result, turns, subscription) = match resumed {
            Ok((result, boundary)) => {
                // A loaded resume reply supersedes unloads received before that reply.
                // Unloads after its boundary still invalidate this recovery.
                if result["thread"]["id"] == thread_id
                    && result["thread"]["canAcceptDirectInput"] == true
                    && matches!(
                        result["thread"]["status"]["type"].as_str(),
                        Some("idle" | "active" | "systemError")
                    )
                {
                    let _current = runtime.current_guard().await?;
                    let mut unloaded = connection.unloaded_threads.lock().await;
                    if unloaded
                        .get(thread_id)
                        .is_some_and(|sequence| *sequence <= boundary)
                    {
                        unloaded.remove(thread_id);
                    }
                }
                let turns = self
                    .turns(&connection, thread_id)
                    .await?
                    .unwrap_or_default();
                (result, turns, SubscriptionState::Available)
            }
            Err(
                error @ Error::Conflict {
                    code: "codex_thread_not_persisted",
                    ..
                },
            ) => {
                // An existing, loaded thread can accept first input even though it has no rollout
                // to resume. Reading metadata does not establish a subscription.
                let result = connection
                    .call(
                        "thread/read",
                        json!({"threadId":thread_id,"includeTurns":false}),
                    )
                    .await?;
                if self.turns(&connection, thread_id).await?.is_some() {
                    return Err(error);
                }
                (result, Vec::new(), SubscriptionState::AwaitingFirstInput)
            }
            Err(error) => return Err(error),
        };
        runtime.current_guard().await?;
        if string(&result["thread"], "id")? != thread_id {
            return Err(Error::ControlUnknown(
                "Codex resumed a different thread".into(),
            ));
        }
        self.reconcile_turns(session, runtime, &turns).await?;
        if connection
            .unloaded_threads
            .lock()
            .await
            .contains_key(thread_id)
        {
            runtime.clear_subscription(session).await;
            return Err(Error::CapabilityUnavailable(
                "Codex thread was unloaded during recovery".into(),
            ));
        }
        self.bind(session, runtime, &result["thread"]).await?;
        if subscription == SubscriptionState::Available {
            self.model_snapshot(session, runtime, &result).await?;
        }
        self.ready(session, runtime, &result["thread"]).await?;
        let unloaded = connection.unloaded_threads.lock().await;
        if unloaded.contains_key(thread_id) {
            runtime.clear_subscription(session).await;
            return Err(Error::CapabilityUnavailable(
                "Codex thread was unloaded during recovery".into(),
            ));
        }
        runtime.set_subscription(session, subscription).await;
        drop(unloaded);
        self.event_ingest.control_available(session);
        let _ = observation_changes().send((runtime.root.clone(), session.into()));
        Ok(())
    }

    pub(super) async fn confirm_control_connection(
        &self,
        target: &pontia_application::runtime::ControlTarget,
        runtime: &CodexRuntime,
    ) -> Result<()> {
        target.validate(&self.pool).await?;
        runtime.current_guard().await?;
        let binding = self.binding(&target.session_id).await?;
        if runtime.subscription(&target.session_id).await != Some(SubscriptionState::Available)
            || runtime
                .connection()
                .await?
                .unloaded_threads
                .lock()
                .await
                .contains_key(&binding.client_session_key)
        {
            return Err(Error::CapabilityUnavailable(
                "Codex control awaits thread subscription and snapshot reconciliation".into(),
            ));
        }
        Ok(())
    }

    async fn session_accepts_control(&self, session: &str) -> Result<bool> {
        let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
            .bind(session)
            .fetch_one(&self.pool)
            .await?;
        Ok(matches!(
            state.as_str(),
            "idle" | "busy" | "waiting_for_interaction"
        ))
    }

    pub(crate) async fn binding(&self, session: &str) -> Result<pontia_application::AgentBinding> {
        AgentBindingService::new(self.pool.clone())
            .binding_for_session(session)
            .await?
            .ok_or_else(|| Error::StateConflict("Codex thread binding is missing".into()))
    }
}

pub(super) fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Domain(format!("Codex response missing {field}")))
}

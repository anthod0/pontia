mod events;
mod models;
mod observer;
pub(crate) mod profile;
#[cfg(test)]
mod subscription_tests;
use crate::runtime::{CodexRuntime, SubscriptionState};
use pontia_application::client_contract::ClientExitOutcome;
use pontia_application::{AgentBindingService, UpsertAgentBindingRequest};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
use tokio::sync::Mutex;

fn confirmed_unsubscribes() -> &'static Mutex<HashSet<String>> {
    static SESSIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SESSIONS.get_or_init(Default::default)
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
                if runtime.subscription(session_id).await != Some(SubscriptionState::Available) {
                    self.subscribe(session_id, &runtime, &binding.client_session_key)
                        .await?;
                }
                let snapshot = connection
                    .call(
                        "thread/read",
                        json!({"threadId":binding.client_session_key,"includeTurns":false}),
                    )
                    .await?;
                let turns = self.turns(&connection, &binding.client_session_key).await?;
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
                let environment: Option<String> = sqlx::query_scalar("SELECT json_extract(metadata,'$.codex_environment') FROM sessions WHERE session_id=?")
                    .bind(session_id).fetch_one(&self.pool).await?;
                let environment: Value = environment
                    .map(|value| serde_json::from_str(&value))
                    .transpose()?
                    .unwrap_or_else(|| json!({}));
                let mut params = json!({"cwd":cwd,"historyMode":"legacy","config":{"shell_environment_policy.set":environment}});
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
                let thread_id = string(&thread, "id")?;
                let snapshot_turns = self.turns(&connection, thread_id).await?;
                self.reconcile_turns(session_id, &runtime, &snapshot_turns)
                    .await?;
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
        turn_id: &str,
    ) -> Result<()> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        self.confirm_control_connection(target, &runtime).await?;
        let binding = self.binding(&target.session_id).await?;
        let native: String = sqlx::query_scalar(
            "SELECT client_turn_id FROM native_turn_bindings WHERE session_id=? AND turn_id=?",
        )
        .bind(&target.session_id)
        .bind(turn_id)
        .fetch_one(&self.pool)
        .await?;
        let connection = runtime.connection().await?;
        let turns = self.turns(&connection, &binding.client_session_key).await?;
        if !turns
            .iter()
            .any(|turn| turn["status"] == "inProgress" && turn["id"] == native)
        {
            return Err(Error::StateConflict(
                "Codex active turn changed before interrupt".into(),
            ));
        }
        connection
            .call(
                "turn/interrupt",
                json!({"threadId":binding.client_session_key,"turnId":native}),
            )
            .await?;
        runtime.current_guard().await?;
        Ok(())
    }

    pub(crate) async fn unsubscribe(
        &self,
        target: &pontia_application::runtime::ControlTarget,
    ) -> Result<ClientExitOutcome> {
        let session_id = &target.session_id;
        let runtime = self.runtime(session_id).await?;
        let _operation = runtime.lock_session(session_id).await;
        target.validate(&self.pool).await?;
        let binding = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?;
        let Some(binding) = binding else {
            if runtime.subscription(session_id).await.is_some() {
                return Err(Error::ControlUnknown(
                    "unbound Codex Session still has subscription state".into(),
                ));
            }
            return Ok(ClientExitOutcome::Confirmed {
                reason: "thread_unsubscribed".into(),
            });
        };
        self.confirm_unsubscribe(session_id, &runtime, &binding.client_session_key)
            .await?;
        Ok(ClientExitOutcome::Confirmed {
            reason: "thread_unsubscribed".into(),
        })
    }

    pub(crate) async fn resume(
        &self,
        target: &pontia_application::runtime::ControlTarget,
    ) -> Result<()> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        target.validate(&self.pool).await?;
        let binding = self.binding(&target.session_id).await?;
        confirmed_unsubscribes()
            .lock()
            .await
            .remove(&target.session_id);
        self.subscribe(&target.session_id, &runtime, &binding.client_session_key)
            .await
    }

    async fn confirm_unsubscribe(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        thread: &str,
    ) -> Result<()> {
        let response = runtime
            .connection()
            .await?
            .call("thread/unsubscribe", json!({"threadId":thread}))
            .await?;
        let _current = runtime.current_guard().await?;
        if !unsubscribe_confirmed(&response) {
            return Err(Error::ControlUnknown(
                "Codex returned an invalid unsubscribe postcondition".into(),
            ));
        }
        runtime
            .set_subscription(session, SubscriptionState::ExitPending)
            .await;
        confirmed_unsubscribes()
            .lock()
            .await
            .insert(session.to_string());
        Ok(())
    }

    pub(super) async fn has_confirmed_unsubscribe(session: &str) -> bool {
        confirmed_unsubscribes().lock().await.contains(session)
    }

    pub(super) async fn clear_confirmed_unsubscribe(session: &str) {
        confirmed_unsubscribes().lock().await.remove(session);
    }

    pub(super) async fn subscribe(
        &self,
        session: &str,
        runtime: &CodexRuntime,
        thread_id: &str,
    ) -> Result<()> {
        runtime
            .set_subscription(session, SubscriptionState::Reconciling)
            .await;
        let connection = runtime.connection().await?;
        let result = connection
            .call(
                "thread/resume",
                self.resume_params(session, thread_id).await?,
            )
            .await?;
        runtime.current_guard().await?;
        if string(&result["thread"], "id")? != thread_id {
            return Err(Error::ControlUnknown(
                "Codex resumed a different thread".into(),
            ));
        }
        self.bind(session, runtime, &result["thread"]).await?;
        self.model_snapshot(session, runtime, &result).await?;
        let turns = self.turns(&connection, thread_id).await?;
        self.reconcile_turns(session, runtime, &turns).await?;
        self.ready(session, runtime, &result["thread"]).await?;
        runtime
            .set_subscription(session, SubscriptionState::Available)
            .await;
        self.event_ingest.control_available(session);
        Ok(())
    }

    pub(super) async fn confirm_control_connection(
        &self,
        target: &pontia_application::runtime::ControlTarget,
        runtime: &CodexRuntime,
    ) -> Result<()> {
        target.validate(&self.pool).await?;
        runtime.current_guard().await?;
        if runtime.subscription(&target.session_id).await != Some(SubscriptionState::Available) {
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
        Ok(matches!(state.as_str(), "idle" | "busy"))
    }

    async fn binding(&self, session: &str) -> Result<pontia_application::AgentBinding> {
        AgentBindingService::new(self.pool.clone())
            .binding_for_session(session)
            .await?
            .ok_or_else(|| Error::StateConflict("Codex thread binding is missing".into()))
    }
}

fn unsubscribe_confirmed(value: &Value) -> bool {
    let status = value
        .as_str()
        .or_else(|| value.get("status").and_then(Value::as_str))
        .or_else(|| value.get("subscriptionStatus").and_then(Value::as_str))
        .or_else(|| value.get("type").and_then(Value::as_str));
    matches!(status, Some("unsubscribed" | "notSubscribed" | "notLoaded"))
}

pub(super) fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Domain(format!("Codex response missing {field}")))
}

mod adoption;
mod background;
mod fork;
mod shutdown;
pub(crate) use background::CodexBackgroundService;
mod compaction;
#[cfg(test)]
mod compaction_tests;
pub(crate) mod environment;
#[cfg(test)]
mod environment_tests;
mod events;
mod exit;
pub(crate) mod instructions;
#[cfg(test)]
mod interaction_tests;
pub(crate) mod interactions;
mod live_output;
#[cfg(test)]
mod live_output_tests;
mod models;
mod observer;
mod proxy_observer;
mod proxy_runtimes;
#[cfg(test)]
mod subscription_tests;
use crate::runtime::{CodexRuntime, CodexServerTarget, SubscriptionState};
use pontia_agent_control::ClientControlStore;
use pontia_agent_control::{InputPayload, UpsertAgentBindingRequest};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
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

fn observation_changes() -> &'static broadcast::Sender<(PathBuf, CodexServerTarget, String)> {
    static CHANGES: OnceLock<broadcast::Sender<(PathBuf, CodexServerTarget, String)>> =
        OnceLock::new();
    CHANGES.get_or_init(|| broadcast::channel(1024).0)
}

pub use observer::CodexObserver;

#[derive(Clone)]
pub struct CodexService {
    pub(super) store: ClientControlStore,
    pub(super) event_ingest: pontia_agent_control::EventIngestService,
}

impl CodexService {
    pub fn new(event_ingest: pontia_agent_control::EventIngestService) -> Self {
        Self {
            store: event_ingest.store(),
            event_ingest,
        }
    }

    pub(super) fn queries(&self) -> pontia_agent_control::ControlQueryService {
        self.store.queries()
    }

    fn native_turns(&self) -> pontia_agent_control::turns::NativeTurnService {
        self.event_ingest.native_turns()
    }

    fn metadata(&self) -> pontia_agent_control::SessionMetadataStore {
        self.store.metadata()
    }

    pub async fn provision(
        &self,
        session_id: &str,
        root: &Path,
        cwd: &Path,
        model: Option<&str>,
        reasoning_effort: Option<&str>,
    ) -> Result<()> {
        let root = root.canonicalize()?;
        self.metadata()
            .patch_keys(
                session_id,
                &[
                    ("codex_control_root", json!(root.display().to_string())),
                    ("codex_launch_cwd", json!(cwd.display().to_string())),
                    ("codex_launch_model", json!(model)),
                    ("codex_launch_reasoning_effort", json!(reasoning_effort)),
                ],
            )
            .await?;
        let _ = observation_changes().send((
            root,
            CodexServerTarget::for_session(&self.store, session_id).await?,
            session_id.into(),
        ));
        Ok(())
    }

    pub(super) async fn root(&self, session_id: &str) -> Result<PathBuf> {
        let root = CodexRuntime::control_root(&self.store, session_id).await?;
        root.ok_or_else(|| Error::StateConflict("Codex control root is missing".into()))
    }

    pub(super) async fn runtime(&self, session_id: &str) -> Result<Arc<CodexRuntime>> {
        let runtime = CodexRuntime::ensure(
            &self.root(session_id).await?,
            CodexServerTarget::for_session(&self.store, session_id).await?,
        )
        .await?;
        runtime
            .instructions_service
            .get_or_init(|| self.instructions());
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
        self.store
            .bindings()
            .upsert_binding(UpsertAgentBindingRequest {
                session_id: session_id.into(),
                client_type: "codex".into(),
                launch_cwd: cwd.into(),
                client_session_key: id.into(),
                client_session_file: thread["path"].as_str().map(str::to_owned),
                metadata: json!({}),
            })
            .await?;
        let _ =
            observation_changes().send((runtime.root.clone(), runtime.target, session_id.into()));
        Ok(())
    }

    pub(crate) async fn start_turn(
        &self,
        target: &pontia_agent_control::runtime::ControlTarget,
        payload: &InputPayload,
    ) -> Result<pontia_agent_control::control::InputReceipt> {
        self.submit(target, payload, false).await
    }

    pub(crate) async fn steer(
        &self,
        target: &pontia_agent_control::runtime::ControlTarget,
        payload: &InputPayload,
    ) -> Result<pontia_agent_control::control::InputReceipt> {
        self.submit(target, payload, true).await
    }

    async fn submit(
        &self,
        target: &pontia_agent_control::runtime::ControlTarget,
        payload: &InputPayload,
        steer: bool,
    ) -> Result<pontia_agent_control::control::InputReceipt> {
        let session_id = &target.session_id;
        let runtime = self.runtime(session_id).await?;
        let _operation = runtime.lock_session(session_id).await;
        self.store.validate_target(target).await?;
        let connection = runtime.connection().await?;
        let binding = self
            .store
            .bindings()
            .binding_for_session(session_id)
            .await?;
        let instructions = self.instructions().codex_binding(session_id).await?;
        let (thread, turns) = match binding {
            Some(binding) => {
                if connection
                    .unloaded_threads
                    .lock()
                    .await
                    .contains_key(&binding.client_session_key)
                    || self.reload_suspended(session_id).await?
                {
                    return Err(Error::StateConflict(
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
                let fields = self
                    .queries()
                    .client_session_fields(session_id)
                    .await?
                    .ok_or_else(Error::missing_record)?;
                let cwd = fields
                    .workspace_ref
                    .as_deref()
                    .or_else(|| fields.metadata["codex_launch_cwd"].as_str())
                    .ok_or_else(|| Error::StateConflict("Codex launch cwd is missing".into()))?;
                let model = fields.metadata["codex_launch_model"].as_str();
                let effort = fields.metadata["codex_launch_reasoning_effort"].as_str();
                let mut config = self.session_config(session_id, &connection, cwd).await?;
                if let Some(effort) = effort {
                    config["model_reasoning_effort"] = effort.into();
                }
                let mut params = json!({"cwd":cwd,"historyMode":"legacy","config":config});
                if let Some(model) = model {
                    params["model"] = model.into();
                }
                runtime
                    .set_subscription(session_id, SubscriptionState::Reconciling)
                    .await;
                let response = match connection.call("thread/start", params).await {
                    Ok(response) => response,
                    Err(error) => {
                        if !matches!(error, Error::ControlUnknown(_)) {
                            runtime.clear_subscription(session_id).await;
                        }
                        return Err(start_rejection(error));
                    }
                };
                runtime.current_guard().await?;
                let thread = response["thread"].clone();
                self.bind(session_id, &runtime, &thread).await?;
                if instructions.is_some() {
                    self.instructions()
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
        let mut params =
            json!({"threadId":thread_id,"input":[{"type":"text","text":payload.text}]});
        let method = match (steer, active) {
            (false, None) => "turn/start",
            (true, Some(active)) => {
                params["expectedTurnId"] = active["id"].clone();
                "turn/steer"
            }
            _ => {
                return Err(Error::Conflict {
                    code: "input_busy",
                    message: "Codex is busy; start input was not submitted".into(),
                });
            }
        };
        let result = connection
            .call(method, params)
            .await
            .map_err(|error| if steer { error } else { start_rejection(error) });
        runtime.current_guard().await?;
        let accepted = result?;
        // turn/start subscribes its connection even when the thread was only loaded in memory.
        if runtime.subscription(session_id).await == Some(SubscriptionState::AwaitingFirstInput) {
            runtime
                .set_subscription(session_id, SubscriptionState::Available)
                .await;
        }
        let _ =
            observation_changes().send((runtime.root.clone(), runtime.target, session_id.into()));
        Ok(pontia_agent_control::control::InputReceipt {
            turn_pending: false,
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
        target: &pontia_agent_control::runtime::ControlTarget,
    ) -> Result<()> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        self.confirm_control_connection(target, &runtime).await?;
        let binding = self.binding(&target.session_id).await?;
        let connection = runtime.connection().await?;
        let turns = self
            .turns(&connection, &binding.client_session_key)
            .await?
            .unwrap_or_default();
        let native = turns
            .iter()
            .find(|turn| turn["status"] == "inProgress")
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

    pub(crate) async fn set_reasoning_effort(
        &self,
        target: &pontia_agent_control::runtime::ControlTarget,
        reasoning_effort: &str,
    ) -> Result<()> {
        let session = &target.session_id;
        let runtime = self.runtime(session).await?;
        let _operation = runtime.lock_session(session).await;
        self.confirm_control_connection(target, &runtime).await?;
        let binding = self.binding(session).await?;
        runtime
            .connection()
            .await?
            .call(
                "thread/settings/update",
                json!({"threadId":binding.client_session_key,"effort":reasoning_effort}),
            )
            .await?;
        runtime.current_guard().await?;
        Ok(())
    }

    pub(crate) async fn resume(
        &self,
        target: &pontia_agent_control::runtime::ControlTarget,
    ) -> Result<()> {
        let runtime = self.runtime(&target.session_id).await?;
        let _operation = runtime.lock_session(&target.session_id).await;
        self.store.validate_target(target).await?;
        let binding = self.binding(&target.session_id).await?;
        self.reconcile_proxy_archive(
            &runtime,
            &target.session_id,
            &binding.client_session_key,
            None,
        )
        .await?;
        confirmed_exits().lock().await.remove(&target.session_id);
        self.subscribe(
            &target.session_id,
            &runtime,
            &binding.client_session_key,
            true,
        )
        .await?;
        self.clear_reload_suspended(&target.session_id).await?;
        Ok(())
    }

    pub(super) async fn has_confirmed_exit(session: &str) -> bool {
        confirmed_exits().lock().await.contains(session)
    }

    async fn reload_suspended(&self, session: &str) -> Result<bool> {
        Ok(self
            .queries()
            .session_metadata(session)
            .await?
            .ok_or_else(Error::missing_record)?["codex_observation_unloaded"]
            == true)
    }

    async fn clear_reload_suspended(&self, session: &str) -> Result<()> {
        self.metadata()
            .remove_keys(session, &["codex_observation_unloaded"])
            .await
    }

    async fn suspend_reload(&self, session: &str) -> Result<()> {
        self.metadata()
            .patch_keys(session, &[("codex_observation_unloaded", json!(true))])
            .await
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
            return Err(Error::Unavailable(
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
            return Err(Error::Unavailable(
                "Codex thread was unloaded during recovery".into(),
            ));
        }
        runtime.set_subscription(session, subscription).await;
        drop(unloaded);
        self.event_ingest.control_available(session);
        let _ = observation_changes().send((runtime.root.clone(), runtime.target, session.into()));
        Ok(())
    }

    pub(super) async fn confirm_control_connection(
        &self,
        target: &pontia_agent_control::runtime::ControlTarget,
        runtime: &CodexRuntime,
    ) -> Result<()> {
        self.store.validate_target(target).await?;
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
            return Err(Error::Unavailable(
                "Codex control awaits thread subscription and snapshot reconciliation".into(),
            ));
        }
        Ok(())
    }

    async fn session_accepts_control(&self, session: &str) -> Result<bool> {
        use pontia_core::domain::SessionState;
        Ok(matches!(
            self.queries()
                .session_state(session)
                .await?
                .ok_or_else(Error::missing_record)?,
            SessionState::Idle | SessionState::Busy | SessionState::WaitingForInteraction
        ))
    }

    pub(crate) async fn binding(
        &self,
        session: &str,
    ) -> Result<pontia_agent_control::AgentBinding> {
        self.store
            .bindings()
            .binding_for_session(session)
            .await?
            .ok_or_else(|| Error::StateConflict("Codex thread binding is missing".into()))
    }
}

fn start_rejection(error: Error) -> Error {
    match error {
        Error::StateConflict(message) => Error::Unavailable(message),
        error => error,
    }
}

pub(super) fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Domain(format!("Codex response missing {field}")))
}

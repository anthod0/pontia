use super::{CodexService, live_output::CodexLiveOutput};
use crate::runtime::{CodexRuntime, SubscriptionState};
use pontia_agent_control::AgentBindingService;
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::{broadcast, watch};

struct Observation {
    threads: HashMap<String, String>,
    output: CodexLiveOutput,
    restored: HashMap<String, u64>,
}

pub struct CodexObserver {
    service: CodexService,
    changes: broadcast::Receiver<(PathBuf, String)>,
    session_events: broadcast::Receiver<DomainEvent>,
    root: PathBuf,
    live_output: pontia_agent_control::LiveOutputService,
    interactions: pontia_agent_control::interactions::InteractionService,
}

impl CodexObserver {
    pub fn new(state: &pontia_agent_control::AgentControl, root: PathBuf) -> Self {
        Self {
            service: CodexService::new(state.event_ingest_service()),
            changes: super::observation_changes().subscribe(),
            session_events: state.agent_events().subscribe(),
            live_output: state.live_output(),
            interactions: state.interactions(),
            root,
        }
    }

    pub async fn prepare(&self) -> Result<()> {
        let root = self.root.canonicalize()?;
        sqlx::query("UPDATE sessions SET metadata=json_set(CASE WHEN json_type(metadata)='object' THEN metadata ELSE '{}' END,'$.codex_control_root',?) WHERE client_type='codex' AND json_extract(metadata,'$.codex_control_root') IS NULL")
            .bind(root.display().to_string())
            .execute(&self.service.pool)
            .await?;
        sqlx::query("UPDATE sessions SET metadata=json_set(metadata,'$.codex_launch_cwd',COALESCE(workspace_ref,(SELECT launch_cwd FROM agent_bindings WHERE agent_bindings.session_id=sessions.session_id),(SELECT json_extract(payload,'$.workspace') FROM events WHERE events.session_id=sessions.session_id AND event_type='session.created' LIMIT 1))) WHERE client_type='codex' AND json_extract(metadata,'$.codex_launch_cwd') IS NULL AND COALESCE(workspace_ref,(SELECT launch_cwd FROM agent_bindings WHERE agent_bindings.session_id=sessions.session_id),(SELECT json_extract(payload,'$.workspace') FROM events WHERE events.session_id=sessions.session_id AND event_type='session.created' LIMIT 1)) IS NOT NULL")
            .execute(&self.service.pool)
            .await?;
        Ok(())
    }

    pub async fn run(mut self, mut shutdown: watch::Receiver<bool>) {
        self.root = match self.root.canonicalize() {
            Ok(root) => root,
            Err(error) => {
                tracing::error!(%error, "Codex observation root is unavailable");
                return;
            }
        };
        loop {
            if *shutdown.borrow() {
                break;
            }
            let active = match sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM sessions WHERE client_type='codex' AND state<>'exited'",
            )
            .fetch_one(&self.service.pool)
            .await
            {
                Ok(active) => active,
                Err(error) => {
                    tracing::error!(%error, "Codex observation could not read Sessions");
                    return;
                }
            };
            if active == 0 {
                tokio::select! {
                    _ = shutdown.changed() => break,
                    change = self.changes.recv() => {
                        match change {
                            Ok((root, _)) if root == self.root => {},
                            Err(broadcast::error::RecvError::Lagged(_)) => {},
                            _ => continue,
                        }
                    }
                }
                continue;
            }
            match CodexRuntime::ensure(&self.root).await {
                Ok(runtime) => {
                    if let Err(error) = self.observe(runtime.clone(), &mut shutdown).await {
                        tracing::warn!(%error, "Codex observation interrupted");
                        if runtime.connection().await.is_ok() {
                            return;
                        }
                    }
                    runtime.clear_subscriptions().await;
                }
                Err(error) => tracing::warn!(%error, "Codex app-server unavailable"),
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        }
    }

    async fn observes(&self, runtime: &CodexRuntime, session: &str) -> Result<bool> {
        Ok(!CodexService::has_confirmed_exit(session).await
            && matches!(
                runtime.subscription(session).await,
                Some(
                    SubscriptionState::Available
                        | SubscriptionState::AwaitingFirstInput
                        | SubscriptionState::Reconciling
                )
            )
            && sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM sessions WHERE session_id=? AND state<>'exited')",
            )
            .bind(session)
            .fetch_one(&self.service.pool)
            .await?)
    }

    async fn reconcile_all(
        &self,
        runtime: &CodexRuntime,
        connection: &Arc<crate::runtime::protocol::Connection>,
        observation: &mut Observation,
    ) -> Result<()> {
        let sessions: Vec<String> = sqlx::query_scalar("SELECT session_id FROM sessions WHERE client_type='codex' AND json_extract(metadata,'$.codex_control_root')=?")
            .bind(runtime.root.display().to_string()).fetch_all(&self.service.pool).await?;
        for session in sessions {
            self.reconcile_session(runtime, connection, &session, true, observation)
                .await?;
        }
        Ok(())
    }

    async fn reconcile_session(
        &self,
        runtime: &CodexRuntime,
        connection: &Arc<crate::runtime::protocol::Connection>,
        session: &str,
        recover: bool,
        observation: &mut Observation,
    ) -> Result<()> {
        let _operation = runtime.lock_session(session).await;
        let row: Option<(String, Option<String>)> = sqlx::query_as("SELECT s.state,a.client_session_key FROM sessions s LEFT JOIN agent_bindings a USING(session_id) WHERE s.session_id=? AND s.client_type='codex' AND json_extract(s.metadata,'$.codex_control_root')=?")
            .bind(session).bind(runtime.root.display().to_string()).fetch_optional(&self.service.pool).await?;
        let Some((state, thread)) = row else {
            return Ok(());
        };
        if let Some(thread) = &thread {
            observation.threads.insert(thread.clone(), session.into());
        }
        let unloaded = connection
            .unloaded_threads
            .lock()
            .await
            .contains_key(thread.as_deref().unwrap_or(""));
        if unloaded {
            self.service.suspend_reload(session).await?;
        }
        if state == "exited"
            || CodexService::has_confirmed_exit(session).await
            || runtime.subscription(session).await == Some(SubscriptionState::ExitPending)
            || unloaded
            || (self.service.reload_suspended(session).await?
                && runtime.subscription(session).await.is_none())
        {
            super::interactions::expire_matching(&self.interactions, connection, |i| {
                i.session == session
            })
            .await?;
            observation.output.invalidate(session);
            observation.restored.remove(session);
            runtime.clear_subscription(session).await;
            if recover
                && state != "exited"
                && let Some(thread) = thread
            {
                // Unloading prevents reload, but persisted history can still confirm a
                // terminal fact lost before the unload notification.
                let result: Result<()> = async {
                    if let Some(turns) = self.service.turns(connection, &thread).await? {
                        self.service
                            .reconcile_turns(session, runtime, &turns)
                            .await?;
                    }
                    Ok(())
                }
                .await;
                if let Err(error) = result {
                    tracing::warn!(%session, %error, "Codex unloaded thread history unavailable");
                }
            }
            return Ok(());
        }
        let Some(thread) = thread else {
            self.service.event_ingest.control_available(session);
            return Ok(());
        };
        let result: Result<()> = async {
            if recover
                && !matches!(
                    runtime.subscription(session).await,
                    Some(SubscriptionState::Available | SubscriptionState::AwaitingFirstInput)
                )
            {
                self.service
                    .subscribe(session, runtime, &thread, false)
                    .await?;
            }
            if runtime.subscription(session).await == Some(SubscriptionState::Available)
                && observation.restored.get(session).copied()
                    != runtime.subscription_revision(session).await
            {
                observation
                    .output
                    .restore(&self.service, runtime, session, &thread)
                    .await?;
                if let Some(revision) = runtime.subscription_revision(session).await {
                    observation.restored.insert(session.into(), revision);
                }
            }
            super::interactions::reconcile(
                &self.interactions,
                &self.service.event_ingest,
                connection,
            )
            .await
        }
        .await;
        if let Err(error) = result {
            observation.output.invalidate(session);
            observation.restored.remove(session);
            if runtime.subscription(session).await == Some(SubscriptionState::Reconciling) {
                runtime.clear_subscription(session).await;
            }
            tracing::warn!(%session, %error, "Codex observation recovery failed");
        }
        Ok(())
    }

    async fn observe(
        &mut self,
        runtime: Arc<CodexRuntime>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        let connection = runtime.connection().await?;
        let result = self
            .observe_connection(runtime, &connection, shutdown)
            .await;
        super::interactions::expire_matching(&self.interactions, &connection, |_| true).await?;
        result
    }

    async fn observe_connection(
        &mut self,
        runtime: Arc<CodexRuntime>,
        connection: &Arc<crate::runtime::protocol::Connection>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        runtime
            .profile_service
            .get_or_init(|| self.service.profiles());
        let mut events = connection.events.subscribe();
        let mut observation = Observation {
            threads: HashMap::new(),
            output: CodexLiveOutput::new(self.live_output.clone()),
            restored: Default::default(),
        };
        self.reconcile_all(&runtime, connection, &mut observation)
            .await?;
        loop {
            tokio::select! {
                _ = shutdown.changed() => return Ok(()),
                change = self.changes.recv() => {
                    match change {
                        Ok((root, session)) if root == runtime.root => {
                            self.reconcile_session(&runtime, connection, &session, false, &mut observation).await?;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            self.reconcile_all(&runtime, connection, &mut observation).await?;
                        }
                        _ => {},
                    }
                }
                event = self.session_events.recv() => {
                    match event {
                        Ok(event) if event.client_type == "codex" && matches!(event.event_type, EventType::SessionExited | EventType::SessionError) => {
                            self.reconcile_session(&runtime, connection, &event.session_id, false, &mut observation).await?;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            self.reconcile_all(&runtime, connection, &mut observation).await?;
                        }
                        _ => {},
                    }
                }
                event = events.recv() => {
                    let event = match event {
                        Ok(event) => event,
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            super::interactions::reconcile(&self.interactions, &self.service.event_ingest, connection).await?;
                            for session in observation.threads.values() { observation.output.invalidate(session); }
                            observation.restored.clear();
                            self.reconcile_all(&runtime, connection, &mut observation).await?;
                            continue;
                        }
                        Err(_) => return Err(Error::CapabilityUnavailable("Codex event stream closed".into())),
                    };
                    if event["method"] == "pontia/disconnected" { return Err(Error::CapabilityUnavailable("Codex disconnected".into())); }
                    let result: Result<()> = async {
                    super::interactions::observe_native(&self.interactions, &self.service.event_ingest, connection).await?;
                    super::interactions::reconcile(&self.interactions, &self.service.event_ingest, connection).await?;
                    let Some(thread) = event.pointer("/params/threadId").and_then(Value::as_str) else { return Ok(()) };
                    let session = match observation.threads.get(thread) {
                        Some(session) => session.clone(),
                        None => match AgentBindingService::new(self.service.pool.clone()).binding_for_client_session("codex", thread).await? {
                            Some(binding) => binding.session_id,
                            None => return Ok(()),
                        },
                    };
                    observation.threads.insert(thread.to_string(), session.clone());
                    if event["method"] == "thread/reverted" {
                        let _operation = runtime.lock_session(&session).await;
                        let _current = runtime.current_guard().await?;
                        observation.output.invalidate(&session);
                        observation.restored.remove(&session);
                        self.service.event_ingest.history_changed(&session);
                        // Successful revert has already reloaded the native thread and kept
                        // its subscriptions. Only retire unloads preceding that confirmation.
                        let mut unloaded = connection.unloaded_threads.lock().await;
                        if unloaded.get(thread).is_some_and(|sequence| *sequence > event.sequence)
                            || runtime.subscription(&session).await == Some(SubscriptionState::ExitPending)
                            || CodexService::has_confirmed_exit(&session).await
                            || !self.service.session_accepts_control(&session).await? {
                            return Ok(());
                        }
                        unloaded.remove(thread);
                        sqlx::query("UPDATE sessions SET metadata=json_remove(metadata,'$.codex_observation_unloaded') WHERE session_id=?")
                            .bind(&session).execute(&self.service.pool).await?;
                        runtime.set_subscription(&session, SubscriptionState::Available).await;
                        drop(unloaded);
                        self.service.event_ingest.control_available(&session);
                        return Ok(());
                    }
                    if connection.unloaded_threads.lock().await.contains_key(thread) {
                        let _operation = runtime.lock_session(&session).await;
                        // Explicit resume may have completed while this event waited for the lock.
                        if !connection.unloaded_threads.lock().await.contains_key(thread) { return Ok(()); }
                        self.service.suspend_reload(&session).await?;
                        observation.output.invalidate(&session);
                        observation.restored.remove(&session);
                        runtime.clear_subscription(&session).await;
                        if event["method"] == "thread/status/changed" && self.service.session_accepts_control(&session).await?
                            && let Some(turns) = self.service.turns(connection, thread).await? {
                            self.service.reconcile_turns(&session, &runtime, &turns).await?;
                        }
                        if event["method"] == "turn/completed" && self.service.session_accepts_control(&session).await? {
                            let _current = runtime.current_guard().await?;
                            self.service.turn_fact(&session, &event["params"]["turn"], "notification").await?;
                        }
                        return Ok(());
                    }
                    if !matches!(runtime.subscription(&session).await, Some(SubscriptionState::Available | SubscriptionState::AwaitingFirstInput | SubscriptionState::Reconciling)) { return Ok(()); }
                    match event["method"].as_str() {
                        Some("thread/tokenUsage/updated") => {
                            let _operation = runtime.lock_session(&session).await;
                            if !self.observes(&runtime, &session).await? { return Ok(()); }
                            let _current = runtime.current_guard().await?;
                            self.service.context_usage_fact(&session, &event["params"]["tokenUsage"]).await?;
                        }
                        Some("thread/settings/updated") => {
                            let _current = runtime.current_guard().await?;
                            self.service.model_fact(&session, &event["params"]["threadSettings"]).await?;
                        }
                        Some("turn/started") => {
                            let _operation = runtime.lock_session(&session).await;
                            if !self.observes(&runtime, &session).await? { return Ok(()); }
                            if let Some(turns) = self.service.turns(connection, thread).await? {
                                self.service.reconcile_turns(&session, &runtime, &turns).await?;
                                if connection.unloaded_threads.lock().await.contains_key(thread) { return Ok(()); }
                                runtime.set_subscription(&session, SubscriptionState::Available).await;
                                observation.output.reconcile(&self.service, &runtime, &session, thread).await;
                                if let Some(revision) = runtime.subscription_revision(&session).await {
                                    observation.restored.insert(session.clone(), revision);
                                }
                            }
                        }
                        Some("item/started" | "item/completed") if event["params"]["item"]["type"] == "contextCompaction" => {
                            self.service.compaction_notification(&runtime, &session, &event).await?;
                        }
                        Some("item/started" | "item/completed") => {
                            let native = super::string(&event["params"], "turnId")?;
                            let bound: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?")
                                .bind(&session).bind(native).fetch_one(&self.service.pool).await?;
                            if bound == 0 {
                                // An empty native start was deferred until its operation
                                // became identifiable. Keep the real native Turn timestamps.
                                let mut turn = self.service.turns(connection, thread).await?
                                    .unwrap_or_default().into_iter().find(|turn| turn["id"] == native)
                                    .unwrap_or_else(|| serde_json::json!({"id":native,"status":"inProgress","items":[]}));
                                if turn["items"].as_array().is_none_or(|items| items.iter().all(|item| item["type"] == "contextCompaction")) {
                                    turn["items"] = serde_json::json!([event["params"]["item"]]);
                                }
                                let _operation = runtime.lock_session(&session).await;
                                let _current = runtime.current_guard().await?;
                                self.service.turn_fact(&session, &turn, "notification").await?;
                            }
                            if let Err(error) = observation.output.tool_call(&self.service, &runtime, &session, &event).await {
                                tracing::warn!(%session, %error, "Codex live output tool call rejected");
                            }
                        }
                        Some("error") => {
                            self.service.compaction_error(&runtime, &session, &event["params"]).await?;
                        }
                        Some("item/commandExecution/outputDelta") => {
                            if let Err(error) = observation.output.tool_output(&self.service, &runtime, &session, &event).await {
                                tracing::warn!(%session, %error, "Codex live tool output rejected");
                            }
                        }
                        Some("item/agentMessage/delta") => {
                            if let Err(error) = observation.output.delta(&self.service, &runtime, &session, &event).await {
                                tracing::warn!(%session, %error, "Codex live output delta rejected");
                            }
                        }
                        Some("item/reasoning/summaryTextDelta") => {
                            if let Err(error) = observation.output.reasoning(&self.service, &runtime, &session, &event).await {
                                tracing::warn!(%session, %error, "Codex live reasoning delta rejected");
                            }
                        }
                        Some("turn/completed") => {
                            let _operation = runtime.lock_session(&session).await;
                            let _current = runtime.current_guard().await?;
                            self.service.turn_fact(&session, &event["params"]["turn"], "notification").await?;
                        }
                        _ => {}
                    }
                    Ok(())
                    }.await;
                    if let Err(error) = result {
                        tracing::warn!(%error, "Codex notification observation failed");
                    }
                }
            }
        }
    }
}

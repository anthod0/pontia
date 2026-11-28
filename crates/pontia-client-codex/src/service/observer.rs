use super::{CodexService, live_output::CodexLiveOutput};
use crate::runtime::{CodexRuntime, CodexServerTarget, SubscriptionState};
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
    title_boundaries: HashMap<String, u64>,
}

pub struct CodexObserver {
    service: CodexService,
    target: CodexServerTarget,
    changes: broadcast::Receiver<(PathBuf, CodexServerTarget, String)>,
    session_events: broadcast::Receiver<DomainEvent>,
    root: PathBuf,
    live_output: pontia_agent_control::LiveOutputService,
    interactions: pontia_agent_control::interactions::InteractionService,
}

impl CodexObserver {
    #[cfg(test)]
    pub fn new(state: &pontia_agent_control::ClientServiceContext, root: PathBuf) -> Self {
        Self::for_target(state, root, CodexServerTarget::User)
    }

    pub fn for_target(
        state: &pontia_agent_control::ClientServiceContext,
        root: PathBuf,
        target: CodexServerTarget,
    ) -> Self {
        Self {
            service: CodexService::new(state.event_ingest_service()),
            target,
            changes: super::observation_changes().subscribe(),
            session_events: state.agent_events().subscribe(),
            live_output: state.live_output(),
            interactions: state.interactions(),
            root,
        }
    }

    pub async fn prepare(&self) -> Result<()> {
        self.root.canonicalize()?;
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
            let active = match self.has_active_sessions().await {
                Ok(active) => active,
                Err(error) => {
                    tracing::error!(%error, "Codex observation could not read Sessions");
                    return;
                }
            };
            if !active {
                tokio::select! {
                    _ = shutdown.changed() => break,
                    change = self.changes.recv() => {
                        match change {
                            Ok((root, target, _)) if root == self.root && target == self.target => {},
                            Err(broadcast::error::RecvError::Lagged(_)) => {},
                            _ => continue,
                        }
                    }
                }
                continue;
            }
            match CodexRuntime::ensure(&self.root, self.target).await {
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

    pub(super) async fn has_active_sessions(&self) -> Result<bool> {
        use pontia_core::domain::SessionState;
        let mut states = vec![
            SessionState::Created,
            SessionState::Starting,
            SessionState::Idle,
            SessionState::Busy,
            SessionState::WaitingForInteraction,
            SessionState::Interrupted,
        ];
        if self.target == CodexServerTarget::User {
            states.push(SessionState::Error);
        }
        let sessions: Vec<String> = self
            .service
            .queries()
            .list_sessions_by_client(
                "codex",
                &states,
                Some(("codex_control_root", self.root.to_string_lossy().as_ref())),
            )
            .await?;
        for session in sessions {
            if self.owns_session(&session).await? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn owns_session(&self, session: &str) -> Result<bool> {
        Ok(CodexServerTarget::for_session(&self.service.store, session).await? == self.target)
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
            && self
                .service
                .queries()
                .session_state(session)
                .await?
                .is_some_and(|state| state != pontia_core::domain::SessionState::Exited))
    }

    async fn reconcile_all(
        &self,
        runtime: &CodexRuntime,
        connection: &Arc<crate::runtime::protocol::Connection>,
        observation: &mut Observation,
    ) -> Result<()> {
        let sessions: Vec<String> = self
            .service
            .queries()
            .list_sessions_by_client(
                "codex",
                &[],
                Some((
                    "codex_control_root",
                    runtime.root.to_string_lossy().as_ref(),
                )),
            )
            .await?;
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
        let Some(fields) = self
            .service
            .queries()
            .client_session_fields(session)
            .await?
        else {
            return Ok(());
        };
        let state = fields.state;
        let thread = fields.client_session_key;
        if fields.client_type != "codex"
            || fields.metadata["codex_control_root"].as_str()
                != Some(runtime.root.to_string_lossy().as_ref())
            || !self.owns_session(session).await?
        {
            return Ok(());
        }
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
        if state == pontia_core::domain::SessionState::Exited
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
                && state != pontia_core::domain::SessionState::Exited
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
                && (recover
                    || observation.restored.get(session).copied()
                        != runtime.subscription_revision(session).await)
            {
                if recover {
                    let (snapshot, boundary) = connection
                        .call_observed(
                            "thread/read",
                            serde_json::json!({"threadId":thread,"includeTurns":false}),
                        )
                        .await?;
                    let _current = runtime.current_guard().await?;
                    if let Some(title) = snapshot["thread"].get("name") {
                        self.service.title_fact(session, title).await?;
                        observation
                            .title_boundaries
                            .insert(session.into(), boundary);
                    }
                }
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
        let associations = connection.interactions.lock().await;
        let waiting = associations
            .iter()
            .map(|(id, native)| (native.session.clone(), id.clone()))
            .collect();
        self.interactions.await_reconnect(waiting).await;
        result
    }

    async fn observe_connection(
        &mut self,
        runtime: Arc<CodexRuntime>,
        connection: &Arc<crate::runtime::protocol::Connection>,
        shutdown: &mut watch::Receiver<bool>,
    ) -> Result<()> {
        runtime
            .instructions_service
            .get_or_init(|| self.service.instructions());
        let mut events = connection.events.subscribe();
        let mut observation = Observation {
            threads: HashMap::new(),
            output: CodexLiveOutput::new(self.live_output.clone()),
            restored: Default::default(),
            title_boundaries: Default::default(),
        };
        self.reconcile_all(&runtime, connection, &mut observation)
            .await?;
        let mut permission_retry = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = permission_retry.tick(), if !connection.interaction_events.lock().await.is_empty() => {
                    super::interactions::observe_native(&self.interactions, &self.service.event_ingest, connection).await?;
                }
                _ = shutdown.changed() => return Ok(()),
                change = self.changes.recv() => {
                    match change {
                        Ok((root, target, session)) if root == runtime.root && target == self.target => {
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
                        Err(_) => return Err(Error::Unavailable("Codex event stream closed".into())),
                    };
                    if event["method"] == "pontia/disconnected" { return Err(Error::Unavailable("Codex disconnected".into())); }
                    let result: Result<()> = async {
                    super::interactions::reconcile(&self.interactions, &self.service.event_ingest, connection).await?;
                    let Some(thread) = event.pointer("/params/threadId").and_then(Value::as_str) else { return Ok(()) };
                    let session = match observation.threads.get(thread) {
                        Some(session) => session.clone(),
                        None => match self.service.store.bindings().binding_for_client_session("codex", thread).await? {
                            Some(binding) if self.owns_session(&binding.session_id).await? => binding.session_id,
                            _ => return Ok(()),
                        },
                    };
                    observation.threads.insert(thread.to_string(), session.clone());
                    if event["method"] == "thread/archived" {
                        self.service.archive_proxy_runtimes(&runtime, &session, thread).await?;
                        return Ok(());
                    }
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
                        self.service.clear_reload_suspended(&session).await?;
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
                        Some("thread/name/updated") => {
                            let _operation = runtime.lock_session(&session).await;
                            if !self.observes(&runtime, &session).await?
                                || observation.title_boundaries.get(&session).is_some_and(|boundary| event.sequence <= *boundary) {
                                return Ok(());
                            }
                            let _current = runtime.current_guard().await?;
                            self.service.title_fact(&session, &event["params"]["threadName"]).await?;
                        }
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
                            let bound = self.service.native_turns().is_bound(&session, native).await?;
                            if !bound {
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
                            if event["params"]["item"]["type"] == "userMessage" {
                                if let Err(error) = observation.output.user_message(&self.service, &runtime, &session, &event).await {
                                    tracing::warn!(%session, %error, "Codex live user input rejected");
                                }
                            } else if let Err(error) = observation.output.tool_call(&self.service, &runtime, &session, &event).await {
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

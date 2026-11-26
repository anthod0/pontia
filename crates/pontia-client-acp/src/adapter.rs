use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::{
    AcpAgents,
    capabilities::{self, AgentBounds},
};
use agent_client_protocol::v1::schema::{
    CancelNotification, ContentBlock, PromptRequest, TextContent,
};
use pontia_agent_control::{
    AgentBindingService, AgentControl, EventIngestService, PontiaEventType,
    client_contract::{
        AgentClientCapabilities, ClientExitMode, ClientExitOutcome, ClientInteractions,
        ClientOperation, ClientService, ClientServicePhase, ClientSession, ClientSessionDetails,
        SessionCreationOption,
    },
    clients::ClientRegistration,
    control::InputReceipt,
    interactions::InteractionResponse,
    runtime::ControlTarget,
    turns::InputIntent,
};
use pontia_core::{Error, Result};
use pontia_runtime::RuntimeStartRequest;
use serde_json::json;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::{
    identity,
    peer::{self, Peer, Peers, Prompt},
    spec::SPEC,
};

pub fn registration() -> ClientRegistration {
    register(AcpAgents::builtin(), None)
}

pub fn registration_with_relay(
    agents: AcpAgents,
    executable: std::path::PathBuf,
) -> ClientRegistration {
    register(agents, Some(executable))
}

fn register(agents: AcpAgents, relay_executable: Option<std::path::PathBuf>) -> ClientRegistration {
    let config = Arc::new(crate::configuration::AcpConfiguration::new(agents));
    let (reconnect, recovery) = tokio::sync::mpsc::unbounded_channel();
    let adapter = Arc::new(AcpAdapter {
        reconnect,
        recovery: Arc::new(Mutex::new(Some(recovery))),
        relay_executable,
        config: config.clone(),
        pool: Arc::new(OnceLock::new()),
        root: Default::default(),
        peers: Arc::new(Mutex::new(HashMap::new())),
        stopped: Arc::new(AtomicBool::new(false)),
        interactions: Arc::new(OnceLock::new()),
    });
    ClientRegistration {
        auth: Some(Arc::new(crate::auth::AcpAuth::new(config.clone()))),
        configuration: Some(config),
        interactions: Some(adapter.clone()),
        events: None,
        service: Some(adapter.clone()),
        instructions: Some(Arc::new(crate::instructions::InstructionsPolicy)),
        in_process: None,
        session: Some(adapter.clone()),
        prepare_on_input: false,
        spec: &SPEC,
        data: Some(adapter),
        launcher: None,
    }
}

#[derive(Clone)]
pub(crate) struct AcpAdapter {
    pub(crate) reconnect: tokio::sync::mpsc::UnboundedSender<String>,
    recovery: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<String>>>>,
    pub(crate) config: Arc<crate::configuration::AcpConfiguration>,
    pub(crate) pool: Arc<OnceLock<SqlitePool>>,
    pub(crate) root: Arc<OnceLock<std::path::PathBuf>>,
    pub(crate) relay_executable: Option<std::path::PathBuf>,
    pub(crate) peers: Peers,
    stopped: Arc<AtomicBool>,
    interactions: Arc<OnceLock<pontia_agent_control::interactions::InteractionService>>,
}

fn unavailable() -> Error {
    Error::CapabilityUnavailable("ACP control connection is unavailable".into())
}

impl AcpAdapter {
    async fn peer(&self, session: &str) -> Result<Arc<Peer>> {
        self.peers
            .lock()
            .await
            .get(session)
            .cloned()
            .ok_or_else(unavailable)
    }
}

impl ClientSession for AcpAdapter {
    fn creation_options(&self) -> ClientOperation<'_, Option<Vec<SessionCreationOption>>> {
        Box::pin(async move {
            let mut options = Vec::new();
            let records = if let Some(pool) = self.pool.get() {
                pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository::new(pool.clone()).list().await?
            } else {
                Vec::new()
            };
            for record in records
                .iter()
                .filter(|row| row.client_type == "acp" && row.enabled)
            {
                let id = &record.client_key;
                let Ok(agent) = self.config.definition(id) else {
                    continue;
                };
                options.push(SessionCreationOption {
                    model_catalog_key: Some(id.clone()),
                    id: serde_json::to_string(&["acp", id])?,
                    name: agent.name.clone(),
                    client_type: "acp".into(),
                    metadata: json!({"acp_agent_id": id}),
                    version: record.version.clone(),
                });
            }
            Ok(Some(options))
        })
    }

    fn provision<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            if self.stopped.load(Ordering::Acquire) {
                return Err(unavailable());
            }
            let root = std::path::absolute(root)?;
            let _ = self.root.set(root.clone());
            let pool = events.db();
            let metadata: String =
                sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id=?")
                    .bind(&request.session_id)
                    .fetch_one(&pool)
                    .await?;
            let metadata: serde_json::Value = serde_json::from_str(&metadata)?;
            let bindings = AgentBindingService::new(pool.clone());
            let binding = bindings.binding_for_session(&request.session_id).await?;
            let bound_identity = binding
                .as_ref()
                .map(|binding| identity::decode(&binding.client_session_key))
                .transpose()?;
            let selected = bound_identity
                .as_ref()
                .map(|identity| identity[0].as_str())
                .or_else(|| metadata["acp_agent_id"].as_str())
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    Error::Domain("metadata.acp_agent_id must be a nonempty string".into())
                })?;
            let definition = self.config.agents.get(selected).ok_or_else(|| {
                Error::CapabilityUnavailable(format!("ACP Agent {selected} is not defined"))
            })?;
            let peers = self.peers.lock().await;
            if self.stopped.load(Ordering::Acquire) {
                return Err(unavailable());
            }
            if peers.contains_key(&request.session_id) {
                return Err(Error::StateConflict(
                    "ACP Session already has a peer".into(),
                ));
            }
            let runtimes: Vec<String> =
                sqlx::query_scalar("SELECT runtime_id FROM session_runtimes WHERE session_id=?")
                    .bind(&request.session_id)
                    .fetch_all(&pool)
                    .await?;
            let runtime = match runtimes.as_slice() {
                [] if binding.is_some() => {
                    return Err(Error::StateConflict(
                        "ACP recovery requires its existing runtime".into(),
                    ));
                }
                [] => request
                    .runtime_id
                    .clone()
                    .ok_or_else(|| Error::Domain("ACP runtime identity is required".into()))?,
                [runtime] => runtime.clone(),
                _ => {
                    return Err(Error::StateConflict(
                        "ACP Session has multiple runtimes".into(),
                    ));
                }
            };
            let cwd = binding
                .as_ref()
                .map(|binding| binding.launch_cwd.clone())
                .or(request.workspace)
                .map(std::path::PathBuf::from)
                .map(Ok)
                .unwrap_or_else(std::env::current_dir)?;
            let mut launch = definition.launch.clone();
            if request.mode == pontia_core::SessionMode::Restricted {
                let files = match definition.restricted_execution {
                    Some(crate::RestrictedExecution::Antigravity) => crate::antigravity::FILES,
                    Some(crate::RestrictedExecution::Kiro) => crate::kiro::restricted::FILES,
                    _ => &[],
                };
                if !files.is_empty() {
                    pontia_agent_control::workspaces::require_managed_launch_directory(
                        &root, &cwd,
                    )?;
                    for file in files {
                        file.install(&cwd)?;
                    }
                }
            }
            if matches!(
                definition.restricted_execution,
                Some(crate::RestrictedExecution::Antigravity)
            ) {
                crate::antigravity::configure_launch(&root, request.mode, &mut launch)?;
            }
            peer::runtime_event(
                &events,
                &request.session_id,
                &runtime,
                PontiaEventType::RuntimeStarting,
                json!({"start_command":peer::start_command(&launch)}),
            )
            .await?;
            let mut config = match peer::resolve_command(&launch, &cwd) {
                Ok(config) => config,
                Err(error) => {
                    peer::runtime_event(
                        &events,
                        &request.session_id,
                        &runtime,
                        PontiaEventType::RuntimeExited,
                        json!({"reason":"executable_unavailable","failure":{"message":error.to_string()}}),
                    )
                    .await?;
                    return Err(error);
                }
            };
            config.env.extend(request.environment);
            if request.mode == pontia_core::SessionMode::Restricted
                && let Some(crate::RestrictedExecution::AgentProfile(profile)) =
                    definition.restricted_execution
            {
                profile.configure_launch(&mut config);
            }
            drop(peers);
            let model_windows = if definition.compaction == Some(crate::CompactionProtocol::Kiro) {
                crate::kiro::model_windows(&config, &cwd)
                    .await
                    .unwrap_or_else(|error| {
                        tracing::warn!(%error, "Kiro context window sizes unavailable");
                        Default::default()
                    })
            } else {
                Default::default()
            };
            let peer = match self
                .launch(&root, &request.session_id, &runtime, &cwd, config)
                .await
            {
                Ok(peer) => peer,
                Err(error) => {
                    peer::runtime_event(
                        &events,
                        &request.session_id,
                        &runtime,
                        PontiaEventType::RuntimeExited,
                        json!({"reason":"spawn_failed","failure":{"message":error.to_string()}}),
                    )
                    .await?;
                    return Err(error);
                }
            };
            let setup = async {
                {
                    let mut state = peer.state.lock().await;
                    state.agent = AgentBounds::of(selected, Some(definition));
                    state.model_windows = model_windows;
                }
                let (native, metadata) = peer.initialize(
                    &cwd,
                    bound_identity.as_ref().map(|identity| identity[1].as_str()),
                    binding.as_ref().map(|binding| &binding.metadata),
                    definition.new_session_instructions_meta_key.as_deref(),
                    request.model.as_deref(),
                ).await?;
                crate::models::select(&peer, request.model.as_deref(), request.reasoning_effort.as_deref(), binding.is_some()).await?;
                let key = identity::encode(selected, &native.to_string())?;
                let mut state = peer.state.lock().await;
                if !state.current || self.stopped.load(Ordering::Acquire) {
                    return Err(unavailable());
                }
                let terminal: bool = sqlx::query_scalar("SELECT state='exited' FROM sessions WHERE session_id=?")
                    .bind(&request.session_id).fetch_one(&pool).await?;
                if terminal { return Err(Error::StateConflict("ACP Session exited during recovery".into())); }
                state.native = Some(native);
                state.blocked = binding.is_some() && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM turns WHERE session_id=? AND state IN ('queued','running'))")
                    .bind(&request.session_id).fetch_one(&pool).await?;
                state.ready = true;
                let params = json!({"context":crate::connection::context(&state),
                    "setup":crate::relay::Observation::Setup {binding:key, metadata, model:state.current_model.clone(),reasoning_effort:crate::models::effort(&state.config_options),cwd:cwd.display().to_string()}});
                drop(state);
                Ok(params)
            }
            .await;
            let params = match setup {
                Ok(params) => params,
                Err(error) => {
                    if let Err(termination_error) = peer.terminate_failed(&error).await {
                        tracing::warn!(session=%request.session_id, %termination_error, "ACP failed startup cleanup failed");
                    }
                    return Err(error);
                }
            };
            if let Err(error) = peer.relay.call("relay.setup", params).await {
                // A lost receipt can leave the immutable setup in the original relay's FIFO.
                // Inspect that relay after reattachment without reissuing native setup commands.
                loop {
                    if self.stopped.load(Ordering::Acquire)
                        || peer.relay.process.observe()
                            == pontia_runtime::ProcessObservation::Exited
                    {
                        return Err(error);
                    }
                    let current = self.peers.lock().await.get(&request.session_id).cloned();
                    if let Some(current) = current {
                        if current.relay.process != peer.relay.process {
                            return Err(error);
                        }
                        if let Ok(status) = current.relay.call("relay.status", json!({})).await {
                            match status["setup_status"].as_str() {
                                Some("confirmed") => {
                                    current.state.lock().await.refresh_capabilities();
                                    break;
                                }
                                Some("pending") => {}
                                _ => return Err(error),
                            }
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
            peer.state.lock().await.refresh_capabilities();
            Ok(())
        })
    }

    fn input<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        input: &'a str,
        turn: Option<&'a str>,
        message: Option<&'a str>,
        intent: &'a InputIntent,
    ) -> ClientOperation<'a, InputReceipt> {
        Box::pin(async move {
            if let InputIntent::Steer { turn_id } = intent {
                let peer = self.peer(&target.session_id).await?;
                return crate::steering::send(&peer, &events, target, turn_id, input, message)
                    .await;
            }
            let turn = turn.ok_or_else(|| {
                Error::Domain("ACP input requires explicit Pontia Turn identity".into())
            })?;
            let peer = self.peer(&target.session_id).await?;
            let reload = {
                let state = peer.state.lock().await;
                state.current
                    && state.active.is_none()
                    && (state.history_error.is_some()
                        || state.steering_history_pending
                        || !state.interjections.is_empty())
            };
            if reload {
                peer.load().await.map_err(|error| match error {
                    Error::Conflict {
                        code:
                            "timeline_history_invalid"
                            | "timeline_source_identity_mismatch"
                            | "timeline_source_retryable",
                        message,
                    } => Error::CapabilityUnavailable(message),
                    error => error,
                })?;
            }
            let mut state = peer.state.lock().await;
            if let Some(profile) = peer.policy.profile
                && state.verified_model.is_none()
            {
                let native = state.native.clone().ok_or_else(unavailable)?;
                drop(state);
                if let Err(error) = profile.verify(&peer, &native).await {
                    peer.terminate().await?;
                    return Err(error);
                }
                state = peer.state.lock().await;
            }
            if peer.policy.profile.is_some()
                && (state.verified_model.is_none()
                    || state.current_model.is_some() && state.current_model != state.verified_model)
            {
                drop(state);
                peer.terminate().await?;
                return Err(Error::CapabilityUnavailable(
                    "ACP Session model changed during verification".into(),
                ));
            }
            target.validate(&events.db()).await?;
            if !state.current || state.closing || state.capabilities.is_none() {
                return Err(unavailable());
            }
            if state.blocked
                || state.promoted.is_some()
                || state.compaction.is_some()
                || state.replay.is_some()
                || state.active.is_some()
            {
                return Err(Error::Conflict {
                    code: "input_busy",
                    message: "ACP prompt is still active".into(),
                });
            }
            let native = state.native.clone().ok_or_else(unavailable)?;
            let prefix = state.prompt_prefix.as_ref().filter(|_| {
                state.system_prompt.is_none()
                    && !state
                        .history
                        .iter()
                        .rev()
                        .take_while(|message| message.role != "context_compaction")
                        .any(|message| message.role == "user")
            });
            let wire_input = match prefix {
                Some(prefix) => format!("{prefix}{input}"),
                None => input.to_owned(),
            };
            let history_index = crate::history::user_bytes(&state.history);
            let prompt_index = state.negotiated.interject.then(|| {
                state
                    .history
                    .iter()
                    .filter_map(|m| m.prompt)
                    .max()
                    .map_or(0, |i| i + 1)
            });
            if let Some(index) = prompt_index {
                state.history.push(crate::history::Message {
                    role: "user",
                    text: input.into(),
                    prompt: Some(index),
                    ..Default::default()
                });
            } else {
                crate::history::append(&mut state.history, "user", input);
            }
            let sent = peer.connection.prepare_request(PromptRequest::new(
                native,
                vec![ContentBlock::Text(TextContent::new(wire_input))],
            ));
            state.active = Some(Prompt {
                request_id: serde_json::to_value(sent.id())?,
                turn: turn.into(),
                started: false,
                sequence: 0,
                raw_results: Default::default(),
                thought: None,
                cancelled: false,
                prompt_index,
                history_index,
                history_end: history_index + input.len(),
            });
            peer.remember(&state, None).await?;
            let key = serde_json::to_string(sent.id())?;
            let session = target.session_id.clone();
            let id = serde_json::to_value(sent.id())?;
            let result_peer = peer.clone();
            sent.on_receiving_result(async move |result| {
                if let Err(error) = peer::finish(&result_peer, &id, result).await {
                    tracing::warn!(%session, %id, %error, "ACP prompt result unavailable");
                }
                Ok(())
            })
            .map_err(peer::protocol_error)?;
            if let Err(error) = peer.receipts.wait(key).await {
                peer.invalidate(&mut state, &error);
                return Err(error);
            }
            state.refresh_capabilities();
            drop(state);
            Ok(InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        })
    }

    fn interrupt<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        turn: Option<&'a str>,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let peer = self.peer(&target.session_id).await?;
            let mut state = peer.state.lock().await;
            target.validate(&events.db()).await?;
            if !state.current || state.closing {
                return Err(unavailable());
            }
            if turn.is_some() && state.active.as_ref().map(|prompt| prompt.turn.as_str()) != turn {
                return Err(Error::StateConflict(
                    "ACP active prompt changed before interrupt".into(),
                ));
            }
            let native = state.native.clone().ok_or_else(unavailable)?;
            peer.connection
                .send_notification(CancelNotification::new(native))
                .map_err(peer::protocol_error)?;
            if let Some(prompt) = state.active.as_mut() {
                prompt.cancelled = true;
            }
            peer.remember(&state, None).await?;
            let permissions = std::mem::take(&mut state.permissions);
            drop(state);
            let sent = peer.receipts.wait("session/cancel".into()).await;
            let expired = crate::interactions::expire(
                &peer.interactions,
                &target.session_id,
                permissions,
                Some(&peer.receipts),
            )
            .await;
            sent.and(expired)
        })
    }

    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move {
            let peer = self.peer(&target.session_id).await?;
            if mode == ClientExitMode::IfIdle {
                let needs_replay = {
                    let state = peer.state.lock().await;
                    state.active.is_none()
                        && state.promoted.is_none()
                        && !state.interjections.is_empty()
                };
                if needs_replay {
                    peer.load().await?;
                }
            }
            let native = {
                let mut state = peer.state.lock().await;
                target.validate(&events.db()).await?;
                if !state.current || state.closing {
                    return Err(unavailable());
                }
                if mode == ClientExitMode::IfIdle
                    && (state.active.is_some()
                        || state.promoted.is_some()
                        || !state.interjections.is_empty()
                        || state.compaction.is_some()
                        || !state.permissions.is_empty()
                        || state.replay.is_some())
                {
                    return Ok(ClientExitOutcome::Busy);
                }
                if !state.ready || state.replay.is_some() {
                    return Err(unavailable());
                }
                state.closing = true;
                state.refresh_capabilities();
                state.native.clone().ok_or_else(unavailable)?
            };
            let session = target.session_id.clone();
            let close = async move {
                peer.close_session(native).await?;
                let exit_key = format!("acp:close:{}", pontia_core::ids::new_event_id());
                peer.terminate().await?;
                peer::fact(
                    &events,
                    &session,
                    &peer.runtime,
                    None,
                    pontia_core::domain::EventType::SessionExited,
                    exit_key,
                    json!({"reason":"acp_session_closed"}),
                )
                .await?;
                Ok(ClientExitOutcome::Requested)
            };
            if mode == ClientExitMode::Unconditional {
                return close.await;
            }
            // The peer owns the in-flight close even after the caller's deadline.
            tokio::time::timeout(std::time::Duration::from_secs(30), tokio::spawn(close))
                .await
                .map_err(|_| Error::ControlUnknown("ACP Session close receipt timed out".into()))?
                .map_err(|error| {
                    Error::ControlUnknown(format!("ACP Session close unconfirmed: {error}"))
                })?
        })
    }

    fn compact<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        operation_id: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let peer = self.peer(&target.session_id).await?;
            target.validate(&events.db()).await?;
            if target.instance()? != peer.runtime {
                return Err(unavailable());
            }
            let operation = operation_id.to_owned();
            tokio::spawn(async move { crate::compaction::request(&peer, operation).await })
                .await
                .map_err(|error| Error::ControlUnknown(error.to_string()))?
        })
    }

    fn set_reasoning_effort<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        reasoning_effort: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let peer = self.peer(&target.session_id).await?;
            target.validate(&events.db()).await?;
            if target.instance()? != peer.runtime {
                return Err(unavailable());
            }
            crate::models::set_reasoning_effort(&peer, reasoning_effort).await
        })
    }

    fn capabilities<'a>(
        &'a self,
        session: &'a str,
        implementation: &'a AgentClientCapabilities,
    ) -> ClientOperation<'a, AgentClientCapabilities> {
        Box::pin(async move {
            // ACP has no capability flag for usage reports. The persisted observation
            // remains evidence of support even when the peer is disconnected.
            let (agent_id, usage) = match self.pool.get() {
                Some(pool) => sqlx::query_as::<_, (Option<String>, Option<String>)>(
                    "SELECT json_extract(metadata, '$.acp_agent_id'), json_extract(metadata, '$.context_usage.confidence') FROM sessions WHERE session_id = ?",
                ).bind(session).fetch_optional(pool).await?.unwrap_or_default(),
                None => Default::default(),
            };
            let peer = self.peer(session).await.ok();
            let state = match &peer {
                Some(peer) => Some(peer.state.lock().await),
                None => None,
            };
            let mut caps = capabilities::effective(
                state.as_ref().and_then(|state| state.capabilities.as_ref()),
                usage.as_deref(),
                implementation,
            );
            if let Some(bounds) = state.as_ref().map(|state| &state.agent.capabilities) {
                caps.timeline = bounds.timeline;
                caps.compaction_history = bounds.compaction_history;
            } else if let Some(id) = agent_id
                && let Ok(definition) = self.config.definition(&id)
            {
                let bounds = AgentBounds::of(&id, Some(definition)).capabilities;
                caps.timeline = bounds.timeline;
                caps.compaction_history = bounds.compaction_history;
            }
            Ok(caps)
        })
    }

    fn available<'a>(&'a self, _pool: SqlitePool, session: &'a str) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Ok(peer) = self.peer(session).await else {
                return Ok(false);
            };
            let state = peer.state.lock().await;
            Ok(state.current
                && !state.closing
                && !state.promotion_pending()
                && state.capabilities.is_some())
        })
    }

    fn details<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async move {
            let binding = AgentBindingService::new(pool.clone())
                .binding_for_session(session)
                .await?;
            let (agent_id, agent_info, system_prompt_injected) = match binding {
                Some(binding) => (
                    Some(identity::decode(&binding.client_session_key)?[0].clone()),
                    binding.metadata["agent_info"].clone(),
                    Some(binding.metadata["system_prompt_injected"] == true),
                ),
                None => (
                    sqlx::query_scalar(
                        "SELECT json_extract(metadata, '$.acp_agent_id') FROM sessions WHERE session_id = ?",
                    )
                    .bind(session)
                    .fetch_one(&pool)
                    .await?,
                    serde_json::Value::Null,
                    None,
                ),
            };
            let name = agent_id.as_ref().map(|id| {
                self.config
                    .agents
                    .get(id)
                    .map_or(id.as_str(), |agent| &agent.name)
            });
            let session_state: String =
                sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
                    .bind(session)
                    .fetch_one(&pool)
                    .await?;
            let runtime_state: Option<String> = sqlx::query_scalar(
                "SELECT state FROM session_runtimes WHERE session_id = ? AND role = 'acp' LIMIT 1",
            )
            .bind(session)
            .fetch_optional(&pool)
            .await?;
            let peer = self.peers.lock().await.get(session).cloned();
            let (connection, in_memory_error) = match peer {
                Some(peer) => {
                    let state = peer.state.lock().await;
                    (
                        if state.current && state.ready {
                            "connected"
                        } else {
                            "reconnecting"
                        },
                        state.connection_error.clone(),
                    )
                }
                None if session_state == "exited" => ("exited", None),
                None if session_state == "error" => ("failed", None),
                None => ("reconnecting", None),
            };
            let persisted_error: Option<String> = sqlx::query_scalar(
                r#"SELECT json_extract(payload, '$.failure.message')
                   FROM events
                   WHERE session_id = ?
                     AND event_type IN ('runtime.exited', 'session.error')
                     AND json_extract(payload, '$.failure.message') IS NOT NULL
                     AND rowid > COALESCE((
                         SELECT MAX(rowid) FROM events
                         WHERE session_id = ? AND event_type = 'runtime.ready'
                     ), 0)
                   ORDER BY rowid DESC
                   LIMIT 1"#,
            )
            .bind(session)
            .bind(session)
            .fetch_optional(&pool)
            .await?
            .flatten();
            let error = in_memory_error.or(persisted_error);
            let connection = if error.is_some() && connection != "connected" {
                if session_state == "error"
                    || session_state == "exited"
                    || runtime_state.as_deref() == Some("exited")
                {
                    "failed"
                } else {
                    "reconnecting"
                }
            } else {
                connection
            };
            Ok(ClientSessionDetails {
                data: json!({"agent_id":agent_id,"name":name,"agent_info":agent_info,"system_prompt_injected":system_prompt_injected,"connection":connection,"connection_error":error}),
            })
        })
    }

    fn resume<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let pool = events.db();
            target.validate(&pool).await?;
            let (model, reasoning_effort): (Option<String>, Option<String>) = sqlx::query_as(
                "SELECT json_extract(metadata, '$.model'), json_extract(metadata, '$.reasoning_effort') FROM sessions WHERE session_id = ?",
            )
            .bind(&target.session_id)
            .fetch_one(&pool)
            .await?;
            self.provision(
                events,
                self.root.get().ok_or_else(unavailable)?,
                RuntimeStartRequest {
                    reasoning_effort,
                    launch_mode: Default::default(),
                    mode: pontia_agent_control::ControlQueryService::new(pool.clone())
                        .session_mode(&target.session_id)
                        .await?,
                    model,
                    session_id: target.session_id.clone(),
                    runtime_id: target.runtime_id.clone(),
                    client_type: "acp".into(),
                    workspace: None,
                    workspace_name: None,
                    handle: None,
                    role: None,
                    start_command: None,
                    environment: pontia_agent_control::ControlQueryService::new(pool.clone())
                        .runtime_environment(&target.session_id)
                        .await?,
                },
            )
            .await
        })
    }
}

impl ClientInteractions for AcpAdapter {
    fn resolve<'a>(
        &'a self,
        _events: EventIngestService,
        session: &'a str,
        interaction: &'a str,
        response: &'a InteractionResponse,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let peer = self.peer(session).await?;
            crate::interactions::resolve(&peer, interaction, response.choice_option_key()?).await
        })
    }
}

impl ClientService for AcpAdapter {
    fn uses_control_socket(&self) -> bool {
        true
    }
    fn connect(
        &self,
        state: AgentControl,
        process_id: Option<u32>,
        rpc: Arc<pontia_agent_control::ipc::rpc::RpcPeer>,
        first: pontia_agent_control::ipc::rpc::RpcRequest,
        requests: pontia_agent_control::ipc::rpc::Requests,
    ) -> ClientOperation<'_, ()> {
        Box::pin(self.attach(state, process_id, rpc, first, requests))
    }
    fn phase(&self) -> ClientServicePhase {
        ClientServicePhase::Transport
    }
    fn start<'a>(
        &'a self,
        state: AgentControl,
    ) -> ClientOperation<'a, Option<tokio::task::JoinHandle<Result<()>>>> {
        Box::pin(async move {
            let root = std::path::absolute(state.pontia_home())?;
            self.root
                .set(root)
                .map_err(|_| Error::StateConflict("ACP service already started".into()))?;
            let _ = self.interactions.set(state.interactions());
            let _ = self.pool.set(state.db());
            let adapter = self.clone();
            let mut shutdown = state.shutdown().subscribe();
            let mut recovery = self.recovery.lock().await.take().ok_or_else(unavailable)?;
            Ok(Some(tokio::spawn(async move {
                let mut recoveries = tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        _ = shutdown.wait_for(|stop| *stop) => break,
                        session = recovery.recv() => {
                            let Some(session) = session else { break; };
                            let adapter = adapter.clone();
                            let state = state.clone();
                            recoveries.spawn(async move {
                                let result = async {
                                    let _launch = pontia_runtime::launch_lock::launch(state.pontia_home())?;
                                    let target = ControlTarget::resolve(&state.db(), &state.clients(), &session, None).await?;
                                    adapter.resume(state.event_ingest_service(), &target).await
                                }.await;
                                if let Err(error) = result { tracing::warn!(%session, %error, "ACP process recovery failed"); }
                            });
                        }
                        _ = recoveries.join_next(), if !recoveries.is_empty() => {}
                    }
                }
                recoveries.abort_all();
                while recoveries.join_next().await.is_some() {}
                adapter.stopped.store(true, Ordering::Release);
                let peers: Vec<_> = adapter
                    .peers
                    .lock()
                    .await
                    .drain()
                    .map(|(_, peer)| peer)
                    .collect();
                for peer in &peers {
                    peer.relay.rpc.close();
                    peer.stop.send_replace(true);
                }
                for peer in peers {
                    peer.stop().await;
                }
                Ok(())
            })))
        })
    }
}

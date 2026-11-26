use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Weak},
};

use crate::{AcpLaunch, history::HistoryIssue};
use agent_client_protocol::{
    Agent, ByteStreams, JsonRpcRequest,
    schema::ProtocolVersion,
    v1::{ConnectionTo, schema::*},
};
use pontia_agent_control::{
    EventIngestService, PontiaEvent, PontiaEventSource, PontiaEventType, ReportedFact,
};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};
use tokio::sync::{Mutex, oneshot, watch};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{
    capabilities::{self, AgentBounds, Live, Negotiated},
    transport::{Receipts, Relay},
};

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct Prompt {
    pub request_id: Value,
    pub turn: String,
    pub started: bool,
    pub sequence: u64,
    // Only IDs of accepted calls; bounded by the shared stream item limit.
    pub raw_results: std::collections::HashSet<String>,
    // Live item extended by consecutive thought chunks; any other agent output ends it.
    pub thought: Option<String>,
    pub cancelled: bool,
    pub history_index: usize,
    pub history_end: usize,
    pub prompt_index: Option<usize>,
}

pub(crate) struct SessionState {
    pub current: bool,
    pub ready: bool,
    pub blocked: bool,
    reconnect_needed: bool,
    pub agent: AgentBounds,
    pub negotiated: Negotiated,
    pub system_prompt: Option<SystemPrompt>,
    load_meta: Option<Meta>,
    pub prompt_prefix: Option<String>,
    pub native: Option<SessionId>,
    pub capabilities: Option<pontia_core::client_capabilities::AgentClientCapabilities>,
    pub current_model: Option<String>,
    pub verified_model: Option<String>,
    pub model_options_available: watch::Sender<bool>,
    pub model_windows: std::collections::BTreeMap<String, u64>,
    pub config_options: Vec<SessionConfigOption>,
    pub active: Option<Prompt>,
    pub kiro_receipts: HashMap<String, oneshot::Sender<Result<()>>>,
    pub interjections: HashMap<String, crate::steering::Interjection>,
    pub promoted: Option<String>,
    pub compaction: Option<crate::compaction::Pending>,
    pub checkpoints: Vec<crate::compaction::Checkpoint>,
    pub checkpoints_need_replay: bool,
    pub closing: bool,
    pub permissions: HashMap<String, crate::interactions::Permission>,
    pub history: Vec<crate::history::Message>,
    pub replay: Option<crate::history::Replay>,
    pub history_error: Option<HistoryIssue>,
    pub history_dirty: bool,
    load_waiters: Option<tokio::sync::broadcast::WeakSender<LoadResult>>,
    pub steering_history_pending: bool,
    pub steered_inputs: String,
    pub cwd: std::path::PathBuf,
}

type LoadResult = std::result::Result<(), (&'static str, String)>;

pub(crate) type Peers = Arc<Mutex<HashMap<String, Arc<Peer>>>>;

pub(crate) struct PeerOwner {
    pub policy: crate::restrictions::SessionPolicy,
    pub agent: AgentBounds,
    pub reconnect: tokio::sync::mpsc::UnboundedSender<String>,
    pub peers: Weak<Mutex<HashMap<String, Arc<Peer>>>>,
}

impl SessionState {
    pub fn needs_history_load(&self) -> bool {
        self.history_dirty
            || self.history_error.is_some()
            || self.checkpoints_need_replay
            || self.steering_history_pending
            || !self.interjections.is_empty()
    }

    fn finish_load(
        &mut self,
        result: std::result::Result<LoadSessionResponse, agent_client_protocol::Error>,
    ) -> Result<()> {
        if !self.current {
            Err(crate::history::unavailable())
        } else {
            let mut replay = self.replay.take().unwrap_or_default();
            replay.finish(self.prompt_prefix.as_deref());
            let result = match result {
                Ok(response) => {
                    if let Some(options) = response.config_options {
                        crate::models::replace(self, options);
                    }
                    Ok(())
                }
                Err(error) => Err(protocol_error(error)),
            };
            if let Err(error) = result {
                self.history_error = Some(HistoryIssue::LoadFailed(format!(
                    "ACP session/load failed: {error}"
                )));
            } else if self.history_error.is_none() && !self.preserves_history(&replay.messages) {
                self.history_error = Some(HistoryIssue::Invalid(
                    "ACP replay did not preserve the known native message history",
                ));
            }
            let result = if let Some(issue) = self.history_error.clone() {
                Err(issue.into())
            } else {
                self.history_dirty = false;
                self.steering_history_pending = false;
                self.steered_inputs.clear();
                self.history = replay.messages;
                self.checkpoints = replay.checkpoints;
                self.checkpoints_need_replay = false;
                Ok(())
            };
            self.refresh_capabilities();
            result
        }
    }

    fn preserves_history(&self, replay: &[crate::history::Message]) -> bool {
        // Antigravity replays still-running steps before all history and rebuilds an
        // interrupted prompt from completed steps, so its tools have no stable position.
        let positioned = |message: &&crate::history::Message| {
            !self.agent.prompt_steering || message.tool.is_none()
        };
        let history: Vec<_> = self.history.iter().filter(positioned).collect();
        let replay: Vec<_> = replay.iter().filter(positioned).collect();
        let mut known = history.as_slice();
        if !self.steered_inputs.is_empty()
            && let Some(start) = known.iter().rposition(|message| message.role == "user")
        {
            let Some(tail) = replay.get(start..) else {
                return false;
            };
            let inputs = tail
                .iter()
                .filter(|m| m.role == "user")
                .flat_map(|m| m.text.bytes());
            if tail.first().is_none_or(|m| m.role != "user")
                || !inputs.eq(known[start].text.bytes().chain(self.steered_inputs.bytes()))
            {
                return false;
            }
            known = &known[..start];
        }
        known
            .iter()
            .take_while(|m| {
                m.prompt
                    .is_none_or(|i| !self.interjections.values().any(|v| v.prompt == i))
            })
            .enumerate()
            .all(|(index, known)| {
                replay.get(index).is_some_and(|loaded| {
                    loaded.role == known.role
                        && (known
                            .tool
                            .as_ref()
                            .zip(loaded.tool.as_ref())
                            .is_some_and(|(a, b)| a.0 == b.0)
                            || loaded.text == known.text
                            || ((known.role == "assistant"
                                || self.steering_history_pending && known.role == "user")
                                && index + 1 == history.len()
                                && loaded.text.starts_with(&known.text)))
                })
            })
    }

    pub fn promotion_pending(&self) -> bool {
        self.promoted.is_some() && self.active.as_ref().is_none_or(|p| !p.request_id.is_null())
    }

    pub fn refresh_capabilities(&mut self) {
        self.capabilities = capabilities::live(
            &self.agent,
            self.negotiated,
            Live {
                current: self.current,
                ready: self.ready,
                has_native: self.native.is_some(),
                closing: self.closing,
                active: self.active.is_some(),
                replaying: self.replay.is_some(),
                blocked: self.blocked || self.compaction.is_some() || self.promotion_pending(),
            },
        );
    }
}

pub(crate) struct Peer {
    pub policy: crate::restrictions::SessionPolicy,
    pub connection: ConnectionTo<Agent>,
    pub relay: Arc<Relay>,
    pub attached: watch::Receiver<bool>,
    pub state: Arc<Mutex<SessionState>>,
    pub receipts: Arc<Receipts>,
    pub registering_permissions: std::sync::Mutex<HashMap<String, watch::Sender<()>>>,
    pub interactions: pontia_agent_control::interactions::InteractionService,
    pub runtime: String,
    pub stop: watch::Sender<bool>,
    pub done: watch::Receiver<bool>,
    pub(crate) events: EventIngestService,
    pub(crate) session: String,
}

pub(crate) fn protocol_error(error: agent_client_protocol::Error) -> Error {
    Error::ControlUnknown(format!("ACP operation could not be confirmed: {error}"))
}

pub(crate) async fn runtime_event(
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    kind: PontiaEventType,
    extra: Value,
) -> Result<()> {
    let mut data = extra;
    data["runtime_id"] = json!(runtime);
    data["role"] = json!("acp");
    events
        .ingest_pontia_event(PontiaEvent::new(
            session,
            None,
            PontiaEventSource::RuntimeManager,
            "acp",
            kind,
            data,
        ))
        .await?;
    Ok(())
}

pub(crate) fn resolve_command(config: &AcpLaunch, cwd: &Path) -> Result<AcpLaunch> {
    let command = Path::new(&config.command);
    let candidates = if command.is_absolute() {
        vec![command.to_path_buf()]
    } else if command.components().count() > 1 {
        vec![cwd.join(command)]
    } else {
        let directories = match config.env.get("PATH") {
            Some(path) => std::env::split_paths(path).collect(),
            None => pontia_runtime::executable::inherited_search_path(None),
        };
        directories
            .into_iter()
            .map(|entry| cwd.join(entry).join(command))
            .collect()
    };
    let executable = candidates
        .into_iter()
        .find(|candidate| {
            if !candidate.is_file() {
                return false;
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                candidate
                    .metadata()
                    .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
            }
            #[cfg(not(unix))]
            {
                true
            }
        })
        .ok_or_else(|| Error::Domain(format!("ACP executable not found: {}", config.command)))?;
    let mut resolved = config.clone();
    // Preserve executable symlinks: virtual environments and multicall binaries
    // can derive their behavior from the invocation path.
    resolved.command = executable
        .to_str()
        .ok_or_else(|| Error::Domain("ACP executable path is not UTF-8".into()))?
        .into();
    Ok(resolved)
}

pub(crate) fn start_command(config: &AcpLaunch) -> String {
    // A diagnostic rendering, never executed through a shell.
    std::iter::once(&config.command)
        .chain(config.args.iter())
        .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

impl Peer {
    pub async fn connect(
        app: &pontia_agent_control::AgentControl,
        identity: &pontia_agent_control::ipc::identity::RuntimeAttachment,
        cwd: &Path,
        relay: Arc<Relay>,
        requests: pontia_agent_control::ipc::rpc::Requests,
        context: Value,
        owner: PeerOwner,
    ) -> Result<Arc<Self>> {
        let events = app.event_ingest_service();
        let interactions = app.interactions();
        let session = identity.session_id.as_str();
        let runtime = identity.runtime_id.as_str();
        let receipts = Receipts::new();
        let (native, forwarded) = tokio::io::duplex(64 * 1024);
        let (reader, writer) = tokio::io::split(native);
        let (outgoing, incoming) = tokio::io::split(forwarded);
        let state = Arc::new(Mutex::new(SessionState {
            current: true,
            ready: false,
            blocked: false,
            reconnect_needed: false,
            native: None,
            capabilities: None,
            agent: owner.agent,
            negotiated: Negotiated::default(),
            system_prompt: None,
            load_meta: None,
            prompt_prefix: None,
            current_model: None,
            verified_model: None,
            config_options: Vec::new(),
            model_options_available: watch::channel(false).0,
            active: None,
            kiro_receipts: Default::default(),
            interjections: HashMap::new(),
            promoted: None,
            closing: false,
            model_windows: Default::default(),
            checkpoints: Vec::new(),
            checkpoints_need_replay: false,
            compaction: None,
            permissions: HashMap::new(),
            history: Vec::new(),
            history_dirty: true,
            load_waiters: None,
            replay: None,
            history_error: Some(HistoryIssue::Pending("ACP history has not been loaded")),
            steering_history_pending: false,
            steered_inputs: String::new(),
            cwd: cwd.to_path_buf(),
        }));
        if !context.is_null() {
            super::connection::restore(&events, session, &mut *state.lock().await, &context)
                .await?;
        }
        let recovered_request = state
            .lock()
            .await
            .active
            .as_ref()
            .map(|prompt| prompt.request_id.clone())
            .filter(|id| !id.is_null());
        let bridge_relay = relay.clone();
        let bridge_receipts = receipts.clone();
        let (ready, attached) = watch::channel(false);
        let associated = Arc::new(std::sync::OnceLock::<Weak<Peer>>::new());
        let bridge = tokio::spawn(crate::transport::bridge(
            bridge_relay,
            requests,
            bridge_receipts,
            (incoming, outgoing),
            associated.clone(),
            recovered_request,
            ready,
        ));
        let permission_peer = associated.clone();
        let recovered_peer = associated.clone();
        let (connected, connection) = oneshot::channel();
        let (stop, mut stopping) = watch::channel(false);
        let (done, completion) = watch::channel(false);
        let update_state = state.clone();
        let update_events = events.clone();
        let update_session = session.to_owned();
        let update_runtime = runtime.to_owned();
        let cleanup_relay = relay.clone();
        let task_runtime = runtime.to_owned();
        let task_state = state.clone();
        let task_receipts = receipts.clone();
        let session = session.to_owned();
        let permission_service = interactions.clone();
        let cleanup_service = interactions.clone();
        let permission_state = state.clone();
        let permission_runtime = runtime.to_owned();
        let permission_session = session.clone();
        let permission_relay = relay.clone();
        let permission_log_session = session.clone();
        let model_events = events.clone();
        let model_session = session.clone();
        tokio::spawn(async move {
            {
                let io = agent_client_protocol::Client
                .v1()
                .on_receive_request(
                    async move |request: RequestPermissionRequest, responder, _cx| {
                        let key = crate::transport::frame_key(request.meta.as_ref());
                        if let Err(error) = crate::interactions::register(
                            &permission_service, &permission_session, &permission_runtime, &permission_relay.rpc, &permission_state, request, responder,
                        ).await {
                            tracing::warn!(session=%permission_log_session, %error, "ACP permission registration failed");
                        }
                        if let Ok(key) = key && let Some(peer) = permission_peer.get().and_then(Weak::upgrade) {
                            peer.registering_permissions.lock().unwrap().remove(&key);
                        }
                        Ok(())
                    },
                    agent_client_protocol::on_receive_request!(),
                )
                .on_receive_notification(
                    async move |message: agent_client_protocol::UntypedMessage, _cx| {
                        if message.method() == "pontia/recovered-result" {
                            if let Some(peer) = recovered_peer.get().and_then(Weak::upgrade) {
                                let result = if let Some(error) = message.params().get("error") {
                                    Err(serde_json::from_value(error.clone())?)
                                } else {
                                    Ok(serde_json::from_value(message.params()["result"].clone())?)
                                };
                                if let Err(error) = finish(&peer, &message.params()["id"], result).await {
                                    tracing::warn!(session=%update_session, %error, "Restored ACP prompt result rejected");
                                }
                            }
                            return Ok(());
                        }
                        if message.method() == "_x.ai/session/update" {
                            crate::compaction::replay(&mut *update_state.lock().await, message.params());
                            return Ok(());
                        }
                        if message.method() != "session/update" {
                            if let Some(peer) = recovered_peer.get().and_then(Weak::upgrade)
                                && let Err(error) = peer.interjection_echo(message.method(), message.params()).await {
                                tracing::warn!(%error, "ACP interjection echo rejected");
                            }
                            return Ok(());
                        }
                        let notification: SessionNotification = match serde_json::from_value(message.params().clone()) {
                            Ok(notification) => notification,
                            Err(error) => {
                                tracing::debug!(session=%update_session, %error, "Ignoring unsupported ACP update");
                                return Ok(());
                            }
                        };
                        if let Err(error) = update(
                            &update_events,
                            &update_session,
                            &update_runtime,
                            &update_state,
                            notification,
                            &message.params()["update"],
                        )
                        .await
                        {
                            tracing::warn!(session=%update_session, %error, "ACP update rejected");
                        }
                        Ok(())
                    },
                    agent_client_protocol::on_receive_notification!(),
                )
                .connect_with(
                    ByteStreams::new(writer.compat_write(), reader.compat()),
                    |cx: ConnectionTo<Agent>| async move {
                        let _ = connected.send(cx.clone());
                        cx.incoming_closed().await;
                        Ok(())
                    },
                );
                tokio::pin!(io);
                tokio::select! {
                    result = &mut io => { if let Err(error) = result { tracing::warn!(%session, %error, "ACP connection ended"); } }
                    _ = stopping.changed() => {}
                }
            } // Drop in-flight handlers before cleanup takes their state lock.
            // Release write waiters before taking the state lock: a control operation
            // may hold that lock while awaiting a pipe flush.
            task_receipts.close();
            cleanup_relay.rpc.close();
            bridge.abort();
            let _ = bridge.await;
            let flush = {
                let state = task_state.lock().await;
                state.current && state.ready && !state.closing && state.replay.is_none()
            };
            if flush && let Some(writer) = events.timeline_cache_writer() {
                writer.flush(&session, None).await;
            }
            let (permissions, recover) = {
                let mut state = task_state.lock().await;
                let recover =
                    state.ready && !state.closing && (state.current || state.reconnect_needed);
                state.current = false;
                state.capabilities = None;
                state.kiro_receipts.clear();
                state.active = None;
                (std::mem::take(&mut state.permissions), recover)
            };
            let waiting = permissions
                .keys()
                .map(|id| (session.clone(), id.clone()))
                .collect();
            cleanup_service.await_reconnect(waiting).await;
            if let Some(output) = events.live_output() {
                output.discard_session(&session);
            }
            // A broken controller connection never implies process exit or authorizes a replacement.
            let mut exited = false;
            for _ in 0..25 {
                if cleanup_relay.process.observe() == pontia_runtime::ProcessObservation::Exited {
                    exited = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            if exited
                && let Err(error) =
                    crate::interactions::expire(&cleanup_service, &session, permissions, None).await
            {
                tracing::warn!(%session,%error,"ACP permission cleanup failed");
            }
            if exited
                && let Err(error) = runtime_event(
                    &events,
                    &session,
                    &task_runtime,
                    PontiaEventType::RuntimeExited,
                    json!({"reason":"peer_exited"}),
                )
                .await
            {
                tracing::warn!(%session, %error, "ACP runtime exit failed");
            }
            if let Some(peers) = owner.peers.upgrade() {
                let mut peers = peers.lock().await;
                if peers
                    .get(&session)
                    .is_some_and(|peer| Arc::ptr_eq(&peer.state, &task_state))
                {
                    peers.remove(&session);
                }
            }
            done.send_replace(true);
            if exited && recover {
                let _ = owner.reconnect.send(session);
            }
        });
        let connection = connection
            .await
            .map_err(|_| Error::ControlUnknown("ACP connection startup failed".into()))?;
        let peer = Arc::new(Self {
            policy: owner.policy,
            relay,
            attached,
            connection,
            state,
            receipts,
            interactions,
            registering_permissions: Default::default(),
            runtime: runtime.into(),
            stop,
            done: completion,
            events: model_events,
            session: model_session,
        });
        let _ = associated.set(Arc::downgrade(&peer));
        Ok(peer)
    }

    pub async fn call<R: JsonRpcRequest>(&self, request: R) -> Result<R::Response> {
        let sent = self.connection.send_request(request);
        let key = serde_json::to_string(sent.id())?;
        self.receipts.wait(key).await?;
        sent.block_task().await.map_err(protocol_error)
    }

    pub async fn close_session(&self, native: SessionId) -> Result<()> {
        if let Some(writer) = self.events.timeline_cache_writer() {
            writer.flush(&self.session, None).await;
        }
        let request = self
            .connection
            .prepare_request(CloseSessionRequest::new(native));
        let key = serde_json::to_string(request.id())?;
        let shared = self.state.clone();
        let (send, receive) = oneshot::channel();
        request
            .on_receiving_result(async move |result| {
                let mut state = shared.lock().await;
                let result = result.map_err(protocol_error).and_then(|_| {
                    if state.current {
                        Ok(())
                    } else {
                        Err(Error::ControlUnknown(
                            "ACP close belongs to an invalid connection".into(),
                        ))
                    }
                });
                state.closing = false;
                if result.is_ok() {
                    state.current = false;
                }
                state.refresh_capabilities();
                let _ = send.send(result);
                Ok(())
            })
            .map_err(protocol_error)?;
        self.receipts.wait(key).await?;
        receive
            .await
            .map_err(|_| Error::ControlUnknown("ACP close response was lost".into()))?
    }

    pub async fn initialize(
        self: &Arc<Self>,
        cwd: &Path,
        existing: Option<&str>,
        binding_metadata: Option<&Value>,
        new_session_instructions_meta_key: Option<&str>,
        model: Option<&str>,
    ) -> Result<(SessionId, Value)> {
        let initialize_request = self.state.lock().await.agent.initialize_request();
        let initialize = self.call(initialize_request).await?;
        if initialize.protocol_version != ProtocolVersion::V1 {
            return Err(Error::CapabilityUnavailable(
                "ACP peer did not negotiate protocol v1".into(),
            ));
        }
        let kiro = {
            let mut state = self.state.lock().await;
            state.negotiated = Negotiated::from_initialize(&initialize, &state.agent)?;
            state.agent.kiro
        };
        if kiro {
            let supported = crate::kiro::steering_supported(self, &initialize).await?;
            self.state.lock().await.negotiated.kiro_steer = supported;
        }
        let instructions = pontia_agent_control::SessionInstructionsService::new(self.events.db())
            .session_instructions(&self.session)
            .await?
            .map(serde_json::from_value::<crate::instructions::Instructions>)
            .transpose()?
            .unwrap_or_default();
        let supported = initialize
            .agent_capabilities
            .session_capabilities
            .system_prompt
            .is_some();
        // Keep the bound instruction format even if a later initialize advertises new capabilities.
        let meta_key = match binding_metadata {
            Some(binding) => binding["new_session_instructions_meta_key"].as_str(),
            None if !supported => crate::instructions::creation_meta_key(
                initialize.agent_capabilities.meta.as_ref(),
                new_session_instructions_meta_key,
            ),
            None => None,
        };
        let first_prompt = binding_metadata
            .map_or(kiro && !supported && meta_key.is_none(), |binding| {
                binding["first_prompt_instructions"] == true
            });
        let inject = match binding_metadata {
            Some(binding) => binding["system_prompt_injected"] == true,
            None => {
                instructions.system_prompt.is_some()
                    && (supported || meta_key.is_some() || first_prompt || instructions.required)
            }
        };
        let prompt_prefix = instructions
            .system_prompt
            .as_deref()
            .filter(|_| first_prompt && inject)
            .map(crate::instructions::prompt_prefix);
        let (system_prompt, new_session_meta) = if inject {
            let text = instructions.system_prompt.ok_or_else(|| {
                Error::StateConflict("ACP Session is missing its bound instructions".into())
            })?;
            match meta_key {
                Some(key) => (None, Some(crate::instructions::prompt_meta(key, text))),
                None if supported => (
                    Some(SystemPrompt::new(vec![ContentBlock::Text(TextContent::new(text))])),
                    None,
                ),
                None if first_prompt => (None, None),
                None => return Err(Error::CapabilityUnavailable(
                    "ACP Agent lacks sessionCapabilities.systemPrompt; create a Session without system_instructions".into(),
                )),
            }
        } else {
            (None, None)
        };
        {
            let mut state = self.state.lock().await;
            state.system_prompt = system_prompt.clone();
            state.prompt_prefix = prompt_prefix;
            // Antigravity rebuilds its custom agent on load, including the appended prompt.
            let mut load_meta = self.policy.meta.clone();
            if meta_key == Some(crate::instructions::ANTIGRAVITY_APPEND)
                && let Some(meta) = &new_session_meta
            {
                load_meta.extend(meta.clone());
            }
            state.load_meta = Some(load_meta);
        }
        let native = match existing {
            Some(id) => SessionId::new(id.to_owned()),
            None => {
                let mut request = NewSessionRequest::new(cwd.to_path_buf());
                request.system_prompt = system_prompt;
                let mut meta = self.policy.meta.clone();
                meta.extend(new_session_meta.into_iter().flatten());
                if let Some(profile) = self.policy.profile {
                    let selected = profile.check_model(self, model).await?;
                    meta.insert(profile.startup_model_key.into(), json!(selected));
                }
                request.meta = (!meta.is_empty()).then_some(meta);
                let response = self.call(request).await?;
                if let Some(profile) = self.policy.profile {
                    profile.verify(self, &response.session_id).await?;
                }
                if let Some(options) = response.config_options {
                    crate::models::replace(&mut *self.state.lock().await, options);
                }
                response.session_id
            }
        };
        if native.to_string().is_empty() {
            return Err(Error::Domain(
                "ACP session/new returned an empty sessionId".into(),
            ));
        }
        let mut metadata = if let Some(info) = initialize.agent_info {
            let mut info = serde_json::to_value(info)?;
            info.as_object_mut()
                .expect("implementation is an object")
                .entry("title")
                .or_insert(Value::Null);
            json!({"agent_info":info})
        } else {
            json!({})
        };
        metadata["system_prompt_injected"] = json!(inject);
        if inject && first_prompt {
            metadata["first_prompt_instructions"] = json!(true);
        }
        if inject && let Some(key) = meta_key {
            metadata["new_session_instructions_meta_key"] = json!(key);
        }
        {
            let mut state = self.state.lock().await;
            if !state.current {
                return Err(crate::history::unavailable());
            }
            state.native = Some(native.clone());
        }
        self.load().await?;
        Ok((native, metadata))
    }

    pub async fn load(self: &Arc<Self>) -> Result<()> {
        let (native, cwd, system_prompt, meta, waiters) = {
            let mut state = self.state.lock().await;
            if !state.current || state.closing {
                return Err(crate::history::pending(
                    "Waiting for ACP session transition",
                ));
            }
            if let Some(waiters) = state
                .load_waiters
                .as_ref()
                .and_then(|flight| flight.upgrade())
            {
                let mut result = waiters.subscribe();
                drop(waiters);
                drop(state);
                return result
                    .recv()
                    .await
                    .map_err(|_| {
                        Error::ControlUnknown("ACP replay ended without a response".into())
                    })?
                    .map_err(|(code, message)| Error::Conflict { code, message });
            }
            if state.active.is_some()
                || state.promoted.is_some()
                || state.compaction.is_some()
                || !state.needs_history_load()
            {
                return Ok(());
            }
            let native = state
                .native
                .clone()
                .ok_or_else(crate::history::unavailable)?;
            // A reattached relay can have an in-flight setup when it connects. Resolve its
            // bound prompt on the first load, after the binding has been recorded.
            let meta = match &state.load_meta {
                Some(meta) => meta.clone(),
                None => {
                    let db = self.events.db();
                    let binding = pontia_agent_control::AgentBindingService::new(db.clone())
                        .binding_for_session(&self.session)
                        .await?
                        .ok_or_else(crate::history::unavailable)?;
                    let mut meta = self.policy.meta.clone();
                    crate::instructions::restore_load_meta(
                        &mut meta,
                        &binding.metadata,
                        &db,
                        &self.session,
                    )
                    .await?;
                    state.load_meta = Some(meta.clone());
                    meta
                }
            };
            let waiters = tokio::sync::broadcast::channel(1).0;
            state.load_waiters = Some(waiters.downgrade());
            state.replay = Some(Default::default());
            state.history_error = None;
            state.refresh_capabilities();
            (
                native,
                state.cwd.clone(),
                state.system_prompt.clone(),
                meta,
                waiters,
            )
        };
        let mut load = LoadSessionRequest::new(native.clone(), cwd);
        load.meta = (!meta.is_empty()).then_some(meta);
        load.system_prompt = system_prompt;
        let request = self.connection.prepare_request(load);
        let key = serde_json::to_string(request.id())?;
        let shared = self.state.clone();
        let peer = self.clone();
        let (send, receive) = oneshot::channel();
        // Completion belongs to the connection, not to the HTTP/read future.
        // Dropping a reader cannot strand replay or let late history become live output.
        let verify = self.policy.profile.is_some();
        request
            .on_receiving_result(async move |result| {
                let complete = async move {
                    if result.is_ok()
                        && let Some(profile) = peer.policy.profile
                        && let Err(error) = profile.verify(&peer, &native).await
                    {
                        {
                            let mut state = shared.lock().await;
                            state.current = false;
                            state.refresh_capabilities();
                        }
                        let _ =
                            waiters.send(Err(("timeline_history_load_failed", error.to_string())));
                        drop(waiters);
                        let _ = send.send(Err(error));
                        let _ = peer.terminate().await;
                        return;
                    }
                    let mut state = shared.lock().await;
                    let mut result = state.finish_load(result);
                    if result.is_ok() && !state.interjections.is_empty() {
                        result = peer.reconcile_interjections(&mut state).await;
                    }
                    state.load_waiters = None;
                    let shared = result.as_ref().map(|_| ()).map_err(|error| match error {
                        Error::Conflict { code, message } => (*code, message.clone()),
                        error => ("timeline_source_retryable", error.to_string()),
                    });
                    let _ = waiters.send(shared);
                    let loaded = result.is_ok();
                    drop(state);
                    let _ = send.send(result);
                    if loaded && let Some(writer) = peer.events.timeline_cache_writer() {
                        tokio::spawn(async move { writer.flush(&peer.session, None).await });
                    }
                };
                // Verification sends another request on this connection. Let its reader
                // dispatch that response while replay keeps new prompts fenced.
                if verify {
                    tokio::spawn(complete);
                } else {
                    complete.await;
                }
                Ok(())
            })
            .map_err(protocol_error)?;
        self.receipts.wait(key).await?;
        receive
            .await
            .map_err(|_| Error::ControlUnknown("ACP history response was lost".into()))?
    }

    pub fn invalidate(&self, state: &mut SessionState) {
        // Native protocol failures fence execution; controller loss alone leaves it running.
        if !self.relay.rpc.is_closed()
            && let Err(error) =
                pontia_runtime::GenericRuntimeManager.terminate_process(&self.relay.process)
        {
            tracing::warn!(session=%self.session, %error, "ACP protocol failure could not stop the process");
        }
        state.reconnect_needed = true;
        state.current = false;
        state.capabilities = None;
        self.stop.send_replace(true);
    }

    pub async fn stop(&self) {
        self.relay.rpc.close();
        self.stop.send_replace(true);
        let mut done = self.done.clone();
        while !*done.borrow_and_update() {
            if done.changed().await.is_err() {
                break;
            }
        }
    }
}

pub(crate) async fn fact(
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    turn: Option<&str>,
    kind: EventType,
    key: String,
    data: Value,
) -> Result<pontia_agent_control::EventIngestResult> {
    let mut data = data;
    data["runtime_id"] = json!(runtime);
    events
        .report_fact(ReportedFact {
            session_id: session.into(),
            turn_id: turn.map(str::to_owned),
            report_key: Some(key),
            fact_type: kind,
            data,
        })
        .await
        .map_err(|error| match error {
            pontia_agent_control::EventReportError::InvalidFact(message) => Error::Domain(message),
            pontia_agent_control::EventReportError::Ingestion(error) => error,
        })
}

async fn update(
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    shared: &Mutex<SessionState>,
    notification: SessionNotification,
    update: &Value,
) -> Result<()> {
    let mut state = shared.lock().await;
    if !state.current {
        return Ok(());
    }
    if state.native.as_ref() != Some(&notification.session_id) {
        if state.replay.is_some() {
            state.history_error = Some(HistoryIssue::IdentityMismatch);
        }
        return Ok(());
    }
    if let SessionUpdate::AvailableCommandsUpdate(ref commands) = notification.update
        && state.agent.compaction == Some(crate::CompactionProtocol::Grok)
    {
        state.negotiated.compact_command = commands
            .available_commands
            .iter()
            .any(|cmd| cmd.name == "compact");
        state.refresh_capabilities();
        return Ok(());
    }
    if let SessionUpdate::ConfigOptionUpdate(config) = notification.update {
        if state.closing {
            return Ok(());
        }
        crate::models::replace(&mut state, config.config_options);
        return Ok(());
    }
    // Retain field presence: schema serialization omits empty collections and
    // JSON null, which are meaningful replacement/return values for live tools.
    let protocol = state.agent.interjection;
    let SessionState { replay, agent, .. } = &mut *state;
    if let Some(replay) = replay {
        if replay.update(agent, update).is_err() {
            state.history_error = Some(HistoryIssue::Invalid(
                "ACP history replay contains unsupported or unassociated message content",
            ));
        }
        return Ok(());
    }
    if agent.compaction == Some(crate::CompactionProtocol::Kiro)
        && let Some(info) = crate::kiro::info(update)
        && info["kind"] == "summarization_completed"
        && info["replay"] != true
    {
        state
            .history
            .push(crate::history::Message::kiro_checkpoint());
    }
    if state.active.is_none() {
        return Ok(());
    }
    if update["sessionUpdate"] != "user_message_chunk"
        && state.history_error.is_none()
        && !state.steering_history_pending
    {
        let _ = crate::history::collect(&mut state.history, update, protocol);
    }
    let prompt = state.active.as_mut().expect("checked active prompt");
    // Only known prompt activity is evidence of started; extension notifications,
    // config changes, usage and unrelated session metadata are not lifecycle facts.
    match update["sessionUpdate"].as_str() {
        Some(
            "agent_message_chunk"
            | "agent_thought_chunk"
            | "tool_call"
            | "tool_call_update"
            | "plan",
        ) => {}
        _ => return Ok(()),
    }
    if let Some(output) = events.live_output() {
        crate::live_output::publish(&output, session, runtime, prompt, update).await?;
    }
    Ok(())
}

pub(crate) async fn finish(
    peer: &Peer,
    id: &Value,
    result: std::result::Result<PromptResponse, agent_client_protocol::Error>,
) -> Result<()> {
    let mut state = peer.state.lock().await;
    if !state.current || state.active.as_ref().map(|prompt| &prompt.request_id) != Some(id) {
        return Ok(());
    }
    if let Err(error) = result {
        peer.invalidate(&mut state);
        return Err(protocol_error(error));
    }
    Ok(())
}

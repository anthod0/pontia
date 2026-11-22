use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Weak},
};

use crate::AcpLaunch;
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
}

pub(crate) struct SessionState {
    pub current: bool,
    pub ready: bool,
    pub blocked: bool,
    reconnect_needed: bool,
    pub agent: AgentBounds,
    pub negotiated: Negotiated,
    pub system_prompt: Option<SystemPrompt>,
    pub native: Option<SessionId>,
    pub capabilities: Option<pontia_core::client_capabilities::AgentClientCapabilities>,
    pub current_model: Option<String>,
    pub config_options: Vec<SessionConfigOption>,
    pub active: Option<Prompt>,
    pub closing: bool,
    pub permissions: HashMap<String, crate::interactions::Permission>,
    pub history: Vec<crate::history::Message>,
    pub replay: Option<Vec<crate::history::Message>>,
    pub history_error: Option<String>,
    pub cwd: std::path::PathBuf,
}

pub(crate) type Peers = Arc<Mutex<HashMap<String, Arc<Peer>>>>;

pub(crate) struct PeerOwner {
    pub agent: AgentBounds,
    pub reconnect: tokio::sync::mpsc::UnboundedSender<String>,
    pub peers: Weak<Mutex<HashMap<String, Arc<Peer>>>>,
}

impl SessionState {
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
                history_ok: self.history_error.is_none(),
                replaying: self.replay.is_some(),
                blocked: self.blocked,
            },
        );
    }
}

pub(crate) struct Peer {
    pub connection: ConnectionTo<Agent>,
    pub relay: Arc<Relay>,
    pub attached: watch::Receiver<bool>,
    pub state: Arc<Mutex<SessionState>>,
    pub receipts: Arc<Receipts>,
    pub registering_permissions: std::sync::Mutex<HashMap<String, watch::Sender<()>>>,
    pub interactions: pontia_agent_control::interactions::InteractionService,
    pub permission_events: crate::interactions::PermissionEvents,
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
        let path = config
            .env
            .get("PATH")
            .map(std::ffi::OsString::from)
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default();
        std::env::split_paths(&path)
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
            current_model: None,
            config_options: Vec::new(),
            active: None,
            closing: false,
            permissions: HashMap::new(),
            history: Vec::new(),
            replay: None,
            history_error: Some("ACP history has not been loaded".into()),
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
            .map(|prompt| prompt.request_id.clone());
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
        let permission_events =
            crate::interactions::PermissionEvents::new(events.clone(), &session, runtime);
        let permission_facts = permission_events.clone();
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
                            &permission_service, &permission_facts, &permission_state, request, responder,
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
                                let turn = peer.state.lock().await.active.as_ref()
                                    .filter(|prompt| prompt.request_id == message.params()["id"])
                                    .map(|prompt| prompt.turn.clone());
                                if let Some(turn) = turn {
                                    let result = if let Some(error) = message.params().get("error") {
                                        Err(serde_json::from_value(error.clone())?)
                                    } else { Ok(serde_json::from_value(message.params()["result"].clone())?) };
                                    if let Err(error) = finish(&update_events, &update_session, &peer, &turn, result).await {
                                        tracing::warn!(session=%update_session, %error, "Restored ACP prompt result rejected");
                                    }
                                }
                            }
                            return Ok(());
                        }
                        if message.method() != "session/update" { return Ok(()); }
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
            let (permissions, recover) = {
                let mut state = task_state.lock().await;
                let recover =
                    state.ready && !state.closing && (state.current || state.reconnect_needed);
                state.current = false;
                state.capabilities = None;
                state.active = None;
                (std::mem::take(&mut state.permissions), recover)
            };
            if let Err(error) =
                crate::interactions::expire(&cleanup_service, &session, permissions, None).await
            {
                tracing::warn!(%session, %error, "ACP permission cleanup failed");
            }
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
            relay,
            attached,
            connection,
            state,
            receipts,
            interactions,
            permission_events,
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
        &self,
        cwd: &Path,
        existing: Option<&str>,
        previously_injected: Option<bool>,
    ) -> Result<(SessionId, Value)> {
        let initialize = self
            .call(InitializeRequest::new(ProtocolVersion::V1))
            .await?;
        if initialize.protocol_version != ProtocolVersion::V1 {
            return Err(Error::CapabilityUnavailable(
                "ACP peer did not negotiate protocol v1".into(),
            ));
        }
        self.state.lock().await.negotiated = Negotiated::from_initialize(&initialize)?;
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
        let inject = match previously_injected {
            Some(injected) => injected,
            None => instructions.system_prompt.is_some() && (supported || instructions.required),
        };
        if inject && !supported {
            return Err(Error::CapabilityUnavailable(
                "ACP Agent lacks sessionCapabilities.systemPrompt; create a Session without system_instructions".into(),
            ));
        }
        let system_prompt = if inject {
            let text = instructions.system_prompt.ok_or_else(|| {
                Error::StateConflict("ACP Session is missing its bound instructions".into())
            })?;
            Some(SystemPrompt::new(vec![ContentBlock::Text(
                TextContent::new(text),
            )]))
        } else {
            None
        };
        self.state.lock().await.system_prompt = system_prompt.clone();
        let native = match existing {
            Some(id) => SessionId::new(id.to_owned()),
            None => {
                let mut request = NewSessionRequest::new(cwd.to_path_buf());
                request.system_prompt = system_prompt;
                let response = self.call(request).await?;
                if let Some(options) = response.config_options {
                    crate::models::replace(
                        &mut *self.state.lock().await,
                        &self.events,
                        &self.session,
                        &self.runtime,
                        options,
                        response.meta.as_ref(),
                    )
                    .await?;
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

    pub async fn load(&self) -> Result<()> {
        let (native, cwd, system_prompt) = {
            let mut state = self.state.lock().await;
            if !state.current || state.closing {
                return Err(crate::history::unavailable());
            }
            if state.active.is_some() {
                return Ok(());
            }
            if state.replay.is_some() {
                return Err(Error::Conflict {
                    code: "timeline_pending",
                    message: "ACP history replay is in progress".into(),
                });
            }
            let native = state
                .native
                .clone()
                .ok_or_else(crate::history::unavailable)?;
            state.replay = Some(Vec::new());
            state.history_error = None;
            state.refresh_capabilities();
            (native, state.cwd.clone(), state.system_prompt.clone())
        };
        let mut load = LoadSessionRequest::new(native, cwd);
        load.system_prompt = system_prompt;
        let request = self.connection.prepare_request(load);
        let key = serde_json::to_string(request.id())?;
        let shared = self.state.clone();
        let events = self.events.clone();
        let session = self.session.clone();
        let runtime = self.runtime.clone();
        let (send, receive) = oneshot::channel();
        // Completion belongs to the connection, not to the HTTP/read future.
        // Dropping a reader cannot strand replay or let late history become live output.
        request
            .on_receiving_result(async move |result| {
                let mut state = shared.lock().await;
                let result = if !state.current {
                    Err(crate::history::unavailable())
                } else {
                    let replay = state.replay.take().unwrap_or_default();
                    let result = match result {
                        Ok(response) => {
                            if let Some(options) = response.config_options {
                                crate::models::replace(
                                    &mut state,
                                    &events,
                                    &session,
                                    &runtime,
                                    options,
                                    response.meta.as_ref(),
                                )
                                .await
                            } else {
                                Ok(())
                            }
                        }
                        Err(error) => Err(protocol_error(error)),
                    };
                    if let Err(error) = result {
                        state.history_error = Some(format!("ACP session/load failed: {error}"));
                    } else if state.history_error.is_none()
                        && !state.history.iter().enumerate().all(|(index, known)| {
                            replay.get(index).is_some_and(|loaded| {
                                loaded.role == known.role
                                    && (loaded.text == known.text
                                        || (known.role == "assistant"
                                            && index + 1 == state.history.len()
                                            && loaded.text.starts_with(&known.text)))
                            })
                        })
                    {
                        state.history_error = Some(
                            "ACP replay did not preserve the known native message history".into(),
                        );
                    }
                    let result = if let Some(reason) = state.history_error.clone() {
                        Err(Error::CapabilityUnavailable(reason))
                    } else {
                        state.history = replay;
                        Ok(())
                    };
                    state.refresh_capabilities();
                    result
                };
                let _ = send.send(result);
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
) -> Result<()> {
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
        })?;
    Ok(())
}

async fn turn_fact(
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    turn: &str,
    kind: EventType,
    data: Value,
) -> Result<()> {
    let key = format!("acp:{turn}:{kind}");
    match fact(events, session, runtime, Some(turn), kind, key, data).await {
        Err(Error::StateConflict(error)) => {
            tracing::warn!(session, turn, %kind, %error, "Discarding conflicting ACP Turn report");
            Ok(())
        }
        result => result,
    }
}

pub(crate) async fn started(
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    prompt: &mut Prompt,
) -> Result<()> {
    if !prompt.started {
        turn_fact(
            events,
            session,
            runtime,
            &prompt.turn,
            EventType::TurnStarted,
            json!({"acp_history_anchor":prompt.history_index}),
        )
        .await?;
        prompt.started = true;
    }
    Ok(())
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
            state.history_error =
                Some("ACP replay returned a different native Session identity".into());
        }
        return Ok(());
    }
    if let SessionUpdate::ConfigOptionUpdate(config) = notification.update {
        if state.closing {
            return Ok(());
        }
        return crate::models::replace(
            &mut state,
            events,
            session,
            runtime,
            config.config_options,
            notification.meta.as_ref(),
        )
        .await;
    }
    if let SessionUpdate::UsageUpdate(usage) = notification.update {
        if state.closing || usage.size == 0 {
            return Ok(());
        }
        return fact(
            events,
            session,
            runtime,
            None,
            EventType::SessionContextUsageUpdated,
            crate::transport::frame_key(notification.meta.as_ref())?,
            json!({"context_usage": {
                "used_tokens": usage.used,
                "max_tokens": usage.size,
                "remaining_tokens": usage.size.saturating_sub(usage.used),
                "usage_ratio": (usage.used as f64 / usage.size as f64).min(1.0),
                "confidence": "exact"
            }}),
        )
        .await;
    }
    // Retain field presence: schema serialization omits empty collections and
    // JSON null, which are meaningful replacement/return values for live tools.
    if let Some(replay) = &mut state.replay {
        if crate::history::collect(replay, update).is_err() {
            state.history_error = Some(
                "ACP history replay contains unsupported or unassociated message content".into(),
            );
        }
        return Ok(());
    }
    if state.active.is_none() {
        return Ok(());
    }
    if update["sessionUpdate"] != "user_message_chunk" && state.history_error.is_none() {
        let _ = crate::history::collect(&mut state.history, update);
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
        ) => started(events, session, runtime, prompt).await?,
        _ => return Ok(()),
    }
    if let Some(output) = events.live_output() {
        crate::live_output::publish(&output, session, runtime, prompt, update).await?;
    }
    Ok(())
}

pub(crate) async fn finish(
    events: &EventIngestService,
    session: &str,
    peer: &Peer,
    turn: &str,
    result: std::result::Result<PromptResponse, agent_client_protocol::Error>,
) -> Result<()> {
    let mut state = peer.state.lock().await;
    if !state.current || state.active.as_ref().map(|prompt| prompt.turn.as_str()) != Some(turn) {
        return Ok(());
    }
    let response = match result {
        Ok(response) => response,
        Err(error) => {
            // A lost/failed response does not prove an execution result. Fence the
            // connection; never allow the unknown prompt to be replayed or bypassed.
            peer.invalidate(&mut state);
            return Err(protocol_error(error));
        }
    };
    let reason = serde_json::to_value(response.stop_reason)?;
    let terminal = match reason.as_str() {
        Some("end_turn") => EventType::TurnCompleted,
        Some("cancelled") => EventType::TurnInterrupted,
        Some("max_tokens" | "max_turn_requests" | "refusal") => EventType::TurnFailed,
        _ => {
            peer.invalidate(&mut state);
            return Err(Error::Domain(format!(
                "Unknown ACP prompt stop reason: {reason}"
            )));
        }
    };
    let prompt = state.active.as_mut().expect("checked active prompt");
    started(events, session, &peer.runtime, prompt).await?;
    // The state lock fences the next submission until this confirmed result has
    // committed and the active prompt has been cleared.
    let data = if terminal == EventType::TurnFailed {
        json!({"failure":{"message":format!("ACP prompt stopped: {}", reason.as_str().unwrap())}})
    } else {
        json!({})
    };
    let mut data = data;
    data["acp_history_anchor"] = json!(prompt.history_end);
    turn_fact(events, session, &peer.runtime, turn, terminal, data).await?;
    state.active = None;
    peer.remember(&state, None).await?;
    let permissions = std::mem::take(&mut state.permissions);
    state.refresh_capabilities();
    drop(state);
    crate::interactions::expire(&peer.interactions, session, permissions, None).await
}

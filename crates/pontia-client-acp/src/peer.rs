use std::{
    collections::HashMap,
    path::Path,
    process::Stdio,
    sync::{Arc, Weak},
};

use agent_client_protocol::{
    Agent, ByteStreams, ConnectionTo, JsonRpcRequest,
    schema::{ProtocolVersion, v1::*},
};
use pontia_application::{
    EventIngestService, LiveOutputBatch, LiveOutputIdentity, LiveOutputProducer, LiveOutputSource,
    LiveOutputUpdate, PontiaEvent, PontiaEventSource, PontiaEventType, ReportedFact,
};
use pontia_config::AcpAgentConfig;
use pontia_core::{Error, Result, domain::EventType};
use serde_json::{Value, json};
use tokio::{
    process::Command,
    sync::{Mutex, oneshot, watch},
};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::{
    spec::ACP_MAX_CAPABILITIES,
    transport::{FlushedWriter, Receipts},
};

pub(crate) struct Prompt {
    pub turn: String,
    pub started: bool,
    pub sequence: u64,
}

pub(crate) struct SessionState {
    pub current: bool,
    pub native: Option<SessionId>,
    pub capabilities: Option<pontia_core::client_capabilities::AgentClientCapabilities>,
    pub active: Option<Prompt>,
    pub closing: bool,
}

pub(crate) type Peers = Arc<Mutex<HashMap<String, Arc<Peer>>>>;

impl SessionState {
    pub fn refresh_capabilities(&mut self) {
        self.capabilities = (self.current && self.native.is_some() && !self.closing).then(|| {
            let mut capabilities = ACP_MAX_CAPABILITIES;
            capabilities.interrupt = self.active.is_some();
            capabilities
        });
    }
}

pub(crate) struct Peer {
    pub connection: ConnectionTo<Agent>,
    pub state: Arc<Mutex<SessionState>>,
    pub receipts: Arc<Receipts>,
    pub runtime: String,
    pub stop: watch::Sender<bool>,
    pub done: watch::Receiver<bool>,
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

pub(crate) fn resolve_command(config: &AcpAgentConfig, cwd: &Path) -> Result<AcpAgentConfig> {
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

pub(crate) fn start_command(config: &AcpAgentConfig) -> String {
    // A diagnostic rendering, never executed through a shell.
    std::iter::once(&config.command)
        .chain(config.args.iter())
        .map(|arg| format!("'{}'", arg.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fingerprint(pid: Option<u32>) -> Option<String> {
    let pid = pid?;
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let start = stat.rsplit_once(") ")?.1.split_whitespace().nth(19)?;
        let boot = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
        Some(json!({"pid":pid,"start_time_ticks":start,"boot_id":boot.trim()}).to_string())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        None
    }
}

impl Peer {
    pub async fn spawn(
        events: EventIngestService,
        session: &str,
        runtime: &str,
        cwd: &Path,
        config: &AcpAgentConfig,
        peers: Weak<Mutex<HashMap<String, Arc<Peer>>>>,
    ) -> Result<Arc<Self>> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .envs(&config.env)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true)
            .spawn()?;
        let process_fingerprint = fingerprint(child.id());
        // Starting again updates this runtime's process incarnation, not its identity.
        if let Err(error) = runtime_event(&events, session, runtime, PontiaEventType::RuntimeStarting,
            json!({"start_command":start_command(config),"process_fingerprint":process_fingerprint})).await {
            let _ = child.kill().await;
            return Err(error);
        }
        let receipts = Receipts::new();
        let writer = FlushedWriter::new(child.stdin.take().expect("piped stdin"), receipts.clone());
        let reader = child.stdout.take().expect("piped stdout");
        let state = Arc::new(Mutex::new(SessionState {
            current: true,
            native: None,
            capabilities: None,
            active: None,
            closing: false,
        }));
        let (connected, connection) = oneshot::channel();
        let (stop, mut stopping) = watch::channel(false);
        let (done, completion) = watch::channel(false);
        let update_state = state.clone();
        let update_events = events.clone();
        let update_session = session.to_owned();
        let update_runtime = runtime.to_owned();
        let task_state = state.clone();
        let task_receipts = receipts.clone();
        let session = session.to_owned();
        let task_runtime = runtime.to_owned();
        tokio::spawn(async move {
            let io = agent_client_protocol::Client
                .builder()
                .on_receive_notification(
                    async move |message: agent_client_protocol::UntypedMessage, _cx| {
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
            // Release write waiters before taking the state lock: a control operation
            // may hold that lock while awaiting a pipe flush.
            task_receipts.close();
            {
                let mut state = task_state.lock().await;
                state.current = false;
                state.capabilities = None;
                state.active = None;
            }
            if let Some(output) = events.live_output() {
                output.discard_session(&session);
            }
            let _ = child.kill().await;
            let _ = child.wait().await;
            if let Err(error) = runtime_event(
                &events,
                &session,
                &task_runtime,
                PontiaEventType::RuntimeExited,
                json!({"reason":"peer_unavailable"}),
            )
            .await
            {
                tracing::warn!(%session, %error, "ACP runtime cleanup failed");
            }
            if let Some(peers) = peers.upgrade() {
                let mut peers = peers.lock().await;
                if peers
                    .get(&session)
                    .is_some_and(|peer| Arc::ptr_eq(&peer.state, &task_state))
                {
                    peers.remove(&session);
                }
            }
            done.send_replace(true);
        });
        let connection = connection
            .await
            .map_err(|_| Error::ControlUnknown("ACP connection startup failed".into()))?;
        Ok(Arc::new(Self {
            connection,
            state,
            receipts,
            runtime: runtime.into(),
            stop,
            done: completion,
        }))
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

    pub async fn initialize(&self, cwd: &Path) -> Result<(SessionId, Value)> {
        let initialize = self
            .call(InitializeRequest::new(ProtocolVersion::V1))
            .await?;
        if initialize.protocol_version != ProtocolVersion::V1 {
            return Err(Error::CapabilityUnavailable(
                "ACP peer did not negotiate protocol v1".into(),
            ));
        }
        let caps = serde_json::to_value(&initialize.agent_capabilities)?;
        let mut missing = Vec::new();
        if caps["loadSession"] != true {
            missing.push("loadSession");
        }
        if !caps
            .pointer("/sessionCapabilities/close")
            .is_some_and(Value::is_object)
        {
            missing.push("sessionCapabilities.close");
        }
        if !missing.is_empty() {
            return Err(Error::CapabilityUnavailable(format!(
                "ACP peer missing required capabilities: {}",
                missing.join(", ")
            )));
        }
        let new = self.call(NewSessionRequest::new(cwd.to_path_buf())).await?;
        if new.session_id.to_string().is_empty() {
            return Err(Error::Domain(
                "ACP session/new returned an empty sessionId".into(),
            ));
        }
        let metadata = if let Some(info) = initialize.agent_info {
            let mut info = serde_json::to_value(info)?;
            info.as_object_mut()
                .expect("implementation is an object")
                .entry("title")
                .or_insert(Value::Null);
            json!({"agent_info":info})
        } else {
            json!({})
        };
        Ok((new.session_id, metadata))
    }

    pub fn invalidate(&self, state: &mut SessionState) {
        state.current = false;
        state.capabilities = None;
        self.stop.send_replace(true);
    }

    pub async fn stop(&self) {
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
    turn: &str,
    kind: EventType,
    data: Value,
) -> Result<()> {
    let mut data = data;
    data["runtime_id"] = json!(runtime);
    events
        .report_fact(ReportedFact {
            session_id: session.into(),
            turn_id: Some(turn.into()),
            report_key: None,
            fact_type: kind,
            data,
        })
        .await
        .map_err(|error| Error::Domain(error.to_string()))?;
    Ok(())
}

pub(crate) async fn started(
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    prompt: &mut Prompt,
) -> Result<()> {
    if !prompt.started {
        fact(
            events,
            session,
            runtime,
            &prompt.turn,
            EventType::TurnStarted,
            json!({}),
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
) -> Result<()> {
    let mut state = shared.lock().await;
    if !state.current || state.native.as_ref() != Some(&notification.session_id) {
        return Ok(());
    }
    let Some(prompt) = &mut state.active else {
        return Ok(());
    };
    let update = serde_json::to_value(notification.update)?;
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
    if update["sessionUpdate"] == "agent_message_chunk"
        && update.pointer("/content/type").and_then(Value::as_str) == Some("text")
        && let Some(text) = update.pointer("/content/text").and_then(Value::as_str)
        && let Some(output) = events.live_output()
    {
        let producer = LiveOutputProducer {
            identity: LiveOutputIdentity {
                session_id: session.into(),
                turn_id: prompt.turn.clone(),
                stream_id: format!("{runtime}:{}", prompt.turn),
            },
            source: LiveOutputSource::RuntimeBound {
                runtime_id: runtime.into(),
            },
        };
        let result = if prompt.sequence == 0 {
            output
                .replace_snapshot(pontia_application::LiveOutputSnapshotReplacement {
                    producer,
                    sequence: 1,
                    items: vec![pontia_application::LiveOutputItem::AssistantText {
                        item_id: "response".into(),
                        text: text.into(),
                    }],
                })
                .await?
        } else {
            output
                .publish_batch(LiveOutputBatch {
                    producer,
                    first_sequence: prompt.sequence + 1,
                    updates: vec![LiveOutputUpdate::AssistantTextDelta {
                        item_id: "response".into(),
                        delta: text.into(),
                    }],
                })
                .await?
        };
        match result {
            pontia_application::LiveOutputPublishOutcome::Accepted {
                accepted_sequence, ..
            } => prompt.sequence = accepted_sequence,
            pontia_application::LiveOutputPublishOutcome::SnapshotRequired { .. } => {
                return Err(Error::CapabilityUnavailable(
                    "ACP live output stream was invalidated".into(),
                ));
            }
        }
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
    fact(events, session, &peer.runtime, turn, terminal, data).await?;
    state.active = None;
    state.refresh_capabilities();
    Ok(())
}

use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use agent_client_protocol::schema::v1::{
    CancelNotification, ContentBlock, PromptRequest, TextContent,
};
use pontia_application::{
    AgentBindingService, AppState, EventIngestService, PontiaEventType, UpsertAgentBindingRequest,
    client_contract::{
        AgentClientCapabilities, ClientExitOutcome, ClientInteractions, ClientOperation,
        ClientService, ClientServicePhase, ClientSession, ClientSessionDetails,
    },
    clients::ClientRegistration,
    control::InputReceipt,
    runtime::ControlTarget,
    sessions::SessionModel,
    turns::InputIntent,
};
use pontia_config::AcpConfig;
use pontia_core::{Error, Result};
use pontia_runtime::{RuntimeStartRequest, RuntimeStartResult};
use serde_json::json;
use sqlx::SqlitePool;
use tokio::sync::Mutex;

use crate::{
    identity,
    peer::{self, Peer, Peers, Prompt},
    spec::SPEC,
};

pub fn registration(config: AcpConfig) -> ClientRegistration {
    let (reconnect, recovery) = tokio::sync::mpsc::unbounded_channel();
    let adapter = Arc::new(AcpAdapter {
        reconnect,
        recovery: Arc::new(Mutex::new(Some(recovery))),
        config,
        peers: Arc::new(Mutex::new(HashMap::new())),
        stopped: Arc::new(AtomicBool::new(false)),
        interactions: Arc::new(OnceLock::new()),
    });
    ClientRegistration {
        interactions: Some(adapter.clone()),
        events: None,
        service: Some(adapter.clone()),
        profile: None,
        in_process: None,
        session: Some(adapter.clone()),
        prepare_on_input: false,
        steer: false,
        spec: &SPEC,
        data: Some(adapter),
        launcher: None,
    }
}

#[derive(Clone)]
pub(crate) struct AcpAdapter {
    config: AcpConfig,
    reconnect: tokio::sync::mpsc::UnboundedSender<String>,
    recovery: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<String>>>>,
    pub(crate) peers: Peers,
    stopped: Arc<AtomicBool>,
    interactions: Arc<OnceLock<pontia_application::interactions::InteractionService>>,
}

fn unavailable() -> Error {
    Error::CapabilityUnavailable("ACP control connection is unavailable".into())
}
fn unsupported() -> Error {
    Error::CapabilityUnavailable("Operation is not implemented by the ACP adapter".into())
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
    fn provision<'a>(
        &'a self,
        events: EventIngestService,
        _root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            if self.stopped.load(Ordering::Acquire) {
                return Err(unavailable());
            }
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
            let config = self
                .config
                .agents
                .get(selected)
                .ok_or_else(|| Error::Domain(format!("Unknown ACP Agent ID: {selected}")))?;
            let mut peers = self.peers.lock().await;
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
            peer::runtime_event(
                &events,
                &request.session_id,
                &runtime,
                PontiaEventType::RuntimeStarting,
                json!({"start_command":peer::start_command(config)}),
            )
            .await?;
            let config = match peer::resolve_command(config, &cwd) {
                Ok(config) => config,
                Err(error) => {
                    peer::runtime_event(
                        &events,
                        &request.session_id,
                        &runtime,
                        PontiaEventType::RuntimeExited,
                        json!({"reason":"executable_unavailable"}),
                    )
                    .await?;
                    return Err(error);
                }
            };
            let peer = match Peer::spawn(
                events.clone(),
                self.interactions.get().cloned().ok_or_else(unavailable)?,
                &request.session_id,
                &runtime,
                &cwd,
                &config,
                peer::PeerOwner {
                    peers: Arc::downgrade(&self.peers),
                    reconnect: self.reconnect.clone(),
                },
            )
            .await
            {
                Ok(peer) => peer,
                Err(error) => {
                    peer::runtime_event(
                        &events,
                        &request.session_id,
                        &runtime,
                        PontiaEventType::RuntimeExited,
                        json!({"reason":"spawn_failed"}),
                    )
                    .await?;
                    return Err(error);
                }
            };
            peers.insert(request.session_id.clone(), peer.clone());
            drop(peers);
            let setup = async {
                let (native, metadata) = peer.initialize(&cwd, bound_identity.as_ref().map(|identity| identity[1].as_str())).await?;
                let key = identity::encode(selected, &native.to_string())?;
                let mut state = peer.state.lock().await;
                if !state.current || self.stopped.load(Ordering::Acquire) {
                    return Err(unavailable());
                }
                let terminal: bool = sqlx::query_scalar("SELECT state='exited' FROM sessions WHERE session_id=?")
                    .bind(&request.session_id).fetch_one(&pool).await?;
                if terminal { return Err(Error::StateConflict("ACP Session exited during recovery".into())); }
                bindings
                    .upsert_binding(UpsertAgentBindingRequest {
                        session_id: request.session_id.clone(),
                        client_type: "acp".into(),
                        launch_cwd: cwd.display().to_string(),
                        client_session_key: key.clone(),
                        client_session_file: None,
                        metadata: metadata.clone(),
                    })
                    .await?;
                state.native = Some(native);
                peer::runtime_event(
                    &events,
                    &request.session_id,
                    &runtime,
                    PontiaEventType::RuntimeReady,
                    json!({}),
                )
                .await?;
                events
                    .report_client_fact(
                        &request.session_id,
                        Some(&runtime),
                        pontia_core::domain::EventType::SessionReady,
                        json!({"client_session_key":key,"launch_cwd":cwd.display().to_string(),"binding_metadata":metadata}),
                    )
                    .await?;
                state.blocked = binding.is_some() && sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM turns WHERE session_id=? AND state IN ('queued','running'))")
                    .bind(&request.session_id).fetch_one(&pool).await?;
                state.ready = true;
                state.refresh_capabilities();
                Ok(())
            }
            .await;
            if setup.is_err() {
                peer.stop().await;
            }
            setup
        })
    }

    fn input<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        input: &'a str,
        turn: Option<&'a str>,
        _message: Option<&'a str>,
        intent: &'a InputIntent,
    ) -> ClientOperation<'a, InputReceipt> {
        Box::pin(async move {
            if !matches!(intent, InputIntent::Start) {
                return Err(unsupported());
            }
            let turn = turn.ok_or_else(|| {
                Error::Domain("ACP input requires explicit Pontia Turn identity".into())
            })?;
            let peer = self.peer(&target.session_id).await?;
            let mut state = peer.state.lock().await;
            target.validate(&events.db()).await?;
            if !state.current || state.closing || state.capabilities.is_none() {
                return Err(unavailable());
            }
            if state.blocked || state.replay.is_some() || state.active.is_some() {
                return Err(Error::Conflict {
                    code: "input_busy",
                    message: "ACP prompt is still active".into(),
                });
            }
            let native = state.native.clone().ok_or_else(unavailable)?;
            let history_index = crate::history::user_bytes(&state.history);
            crate::history::append(&mut state.history, "user", input);
            state.active = Some(Prompt {
                turn: turn.into(),
                started: false,
                sequence: 0,
                cancelled: false,
                history_index,
                history_end: history_index + input.len(),
            });
            let sent = peer.connection.prepare_request(PromptRequest::new(
                native,
                vec![ContentBlock::Text(TextContent::new(input))],
            ));
            let key = serde_json::to_string(sent.id())?;
            let session = target.session_id.clone();
            let turn = turn.to_owned();
            let result_peer = peer.clone();
            sent.on_receiving_result(async move |result| {
                if let Err(error) =
                    peer::finish(&events, &session, &result_peer, &turn, result).await
                {
                    tracing::warn!(%session, %turn, %error, "ACP prompt result unavailable");
                }
                Ok(())
            })
            .map_err(peer::protocol_error)?;
            if let Err(error) = peer.receipts.wait(key).await {
                peer.invalidate(&mut state);
                return Err(error);
            }
            state.refresh_capabilities();
            drop(state);
            Ok(InputReceipt {
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        })
    }

    fn interrupt<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        turn: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let peer = self.peer(&target.session_id).await?;
            let mut state = peer.state.lock().await;
            target.validate(&events.db()).await?;
            if !state.current || state.closing {
                return Err(unavailable());
            }
            if state.active.as_ref().map(|prompt| prompt.turn.as_str()) != Some(turn) {
                return Err(Error::StateConflict(
                    "ACP active prompt changed before interrupt".into(),
                ));
            }
            let native = state.native.clone().ok_or_else(unavailable)?;
            peer.connection
                .send_notification(CancelNotification::new(native))
                .map_err(peer::protocol_error)?;
            state.active.as_mut().expect("checked prompt").cancelled = true;
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
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move {
            let peer = self.peer(&target.session_id).await?;
            let native = {
                let mut state = peer.state.lock().await;
                target.validate(&events.db()).await?;
                if !state.current || !state.ready || state.closing || state.replay.is_some() {
                    return Err(unavailable());
                }
                state.closing = true;
                state.refresh_capabilities();
                state.native.clone().ok_or_else(unavailable)?
            };
            // The ordered close callback fences late updates before waking us.
            // A subsequent EOF cannot undo its confirmed close result.
            peer.close_session(native).await?;
            peer.stop().await;
            self.peers.lock().await.remove(&target.session_id);
            Ok(ClientExitOutcome::Confirmed {
                reason: "acp_session_closed".into(),
            })
        })
    }

    fn capabilities<'a>(
        &'a self,
        session: &'a str,
        implementation: &'a AgentClientCapabilities,
    ) -> ClientOperation<'a, AgentClientCapabilities> {
        Box::pin(async move {
            let Ok(peer) = self.peer(session).await else {
                return Ok(AgentClientCapabilities::default());
            };
            let retry_history = {
                let state = peer.state.lock().await;
                state.current
                    && state.ready
                    && state.history_error.is_some()
                    && state.replay.is_none()
            };
            if retry_history {
                let _ = peer.load().await;
            }
            let state = peer.state.lock().await;
            Ok(state
                .capabilities
                .as_ref()
                .map(|caps| caps.intersection(implementation))
                .unwrap_or_default())
        })
    }

    fn available<'a>(&'a self, _pool: SqlitePool, session: &'a str) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Ok(peer) = self.peer(session).await else {
                return Ok(false);
            };
            let state = peer.state.lock().await;
            Ok(state.current && !state.closing && state.capabilities.is_some())
        })
    }

    fn details<'a>(
        &'a self,
        pool: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async move {
            let binding = AgentBindingService::new(pool)
                .binding_for_session(session)
                .await?;
            let (agent_id, agent_info) = match binding {
                Some(binding) => (
                    Some(identity::decode(&binding.client_session_key)?[0].clone()),
                    binding.metadata["agent_info"].clone(),
                ),
                None => (None, serde_json::Value::Null),
            };
            let name = agent_id
                .as_ref()
                .and_then(|id| self.config.agents.get(id))
                .and_then(|agent| agent.name.clone())
                .or_else(|| agent_id.clone());
            Ok(ClientSessionDetails {
                data: json!({"agent_id":agent_id,"name":name,"agent_info":agent_info}),
                model_control_unavailable_reason: Some(
                    "ACP model control is not implemented".into(),
                ),
            })
        })
    }

    fn resume<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            target.validate(&events.db()).await?;
            self.provision(
                events,
                Path::new("."),
                RuntimeStartRequest {
                    session_id: target.session_id.clone(),
                    runtime_id: target.runtime_id.clone(),
                    client_type: "acp".into(),
                    workspace: None,
                    workspace_name: None,
                    handle: None,
                    role: None,
                    start_command: None,
                    environment: Default::default(),
                },
            )
            .await
        })
    }
    fn list_models<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
    ) -> ClientOperation<'a, Vec<SessionModel>> {
        Box::pin(async { Err(unsupported()) })
    }
    fn set_model<'a>(
        &'a self,
        _events: EventIngestService,
        _target: &'a ControlTarget,
        _model: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async { Err(unsupported()) })
    }
    fn open_interface<'a>(
        &'a self,
        _events: EventIngestService,
        _root: &'a Path,
        _session: &'a str,
        _runtime: &'a str,
    ) -> ClientOperation<'a, RuntimeStartResult> {
        Box::pin(async { Err(unsupported()) })
    }
}

impl ClientInteractions for AcpAdapter {
    fn resolve<'a>(
        &'a self,
        _events: EventIngestService,
        session: &'a str,
        interaction: &'a str,
        option: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let peer = self.peer(session).await?;
            crate::interactions::resolve(&peer.state, &peer.receipts, interaction, option).await
        })
    }
}

impl ClientService for AcpAdapter {
    fn phase(&self) -> ClientServicePhase {
        ClientServicePhase::Transport
    }
    fn start<'a>(
        &'a self,
        state: AppState,
    ) -> ClientOperation<'a, Option<tokio::task::JoinHandle<Result<()>>>> {
        Box::pin(async move {
            let _ = self.interactions.set(state.interactions());
            let adapter = self.clone();
            let mut shutdown = state.shutdown().subscribe();
            let mut recovery = self.recovery.lock().await.take().ok_or_else(unavailable)?;
            // Recover only persisted ACP bindings. Never create native sessions on startup.
            let sessions: Vec<String> = sqlx::query_scalar("SELECT s.session_id FROM sessions s JOIN agent_bindings b USING(session_id) WHERE b.client_type='acp' AND s.state IN ('starting','idle','busy','interrupted')")
                .fetch_all(&state.db()).await?;
            for session in sessions {
                let _ = adapter.reconnect.send(session);
            }
            Ok(Some(tokio::spawn(async move {
                let mut recoveries = tokio::task::JoinSet::new();
                loop {
                    tokio::select! {
                        _ = async { let _ = shutdown.wait_for(|stop| *stop).await; } => break,
                        session = recovery.recv() => {
                            let Some(session) = session else { break; };
                            let adapter = adapter.clone();
                            let state = state.clone();
                            recoveries.spawn(async move {
                            let recover = async {
                                let target = ControlTarget::resolve(&state.db(), &state.clients(), &session, None).await?;
                                let active: bool = sqlx::query_scalar("SELECT state IN ('starting','idle','busy','interrupted') FROM sessions WHERE session_id=?")
                                    .bind(&session).fetch_one(&state.db()).await?;
                                if active { adapter.resume(state.event_ingest_service(), &target).await?; }
                                Ok::<_, Error>(())
                            };
                            if let Err(error) = recover.await { tracing::warn!(%session, %error, "ACP reconnect unavailable"); }
                            });
                        }
                        result = recoveries.join_next(), if !recoveries.is_empty() => {
                            if let Some(Err(error)) = result { tracing::warn!(%error, "ACP recovery task failed"); }
                        }
                    }
                }
                adapter.stopped.store(true, Ordering::Release);
                recoveries.abort_all();
                while recoveries.join_next().await.is_some() {}
                let peers: Vec<_> = adapter
                    .peers
                    .lock()
                    .await
                    .drain()
                    .map(|(_, peer)| peer)
                    .collect();
                for peer in &peers {
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

mod connection;
mod history;
mod instructions;
mod interactions;
mod models;
mod output;
mod peer;
mod reports;
mod resume;
pub mod setup;
mod spec;
mod transport;

use peer::{Input, Peer};
use pontia_agent_control::{
    AgentControl, EventIngestService, client_contract::*, clients::ClientRegistration,
    control::InputReceipt, runtime::ControlTarget, turns::InputIntent,
};
use pontia_core::{Error, Result};
use pontia_runtime::RuntimeStartRequest;
use serde_json::{Value, json};
use sqlx::SqlitePool;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
use tokio::sync::Mutex;

pub fn registration() -> ClientRegistration {
    registration_with_host(std::env::var_os("PONTIA_CLAUDE_HOST_PATH").map(PathBuf::from))
}

pub fn registration_with_host(host: Option<PathBuf>) -> ClientRegistration {
    let adapter = Arc::new(Adapter {
        host,
        interactions: Arc::new(OnceLock::new()),
        peers: Default::default(),
        stopped: Arc::new(AtomicBool::new(false)),
        context: Arc::new(OnceLock::new()),
    });
    ClientRegistration {
        configuration: Some(Arc::new(Configuration {
            host: adapter.host.clone(),
        })),
        interactions: Some(adapter.clone()),
        events: Some(adapter.clone()),
        service: Some(adapter.clone()),
        instructions: Some(Arc::new(instructions::InstructionsPolicy)),
        in_process: None,
        session: Some(adapter.clone()),
        prepare_on_input: true,
        spec: &spec::SPEC,
        data: Some(adapter),
        launcher: None,
    }
}

struct Configuration {
    host: Option<PathBuf>,
}
impl ClientConfiguration for Configuration {
    fn restricted_execution_policy(
        &self,
        _key: &str,
    ) -> Option<pontia_core::execution_policy::ExecutionPolicy> {
        Some(pontia_core::execution_policy::ExecutionPolicy {
            disabled_tools: [
                "Edit",
                "Write",
                "NotebookEdit",
                "Agent",
                "Task",
                "SendMessage",
                "TeamCreate",
                "TeamDelete",
                "Workflow",
                "EnterWorktree",
                "ExitWorktree",
                "CronCreate",
                "CronDelete",
                "ScheduleWakeup",
                "RemoteTrigger",
                "Monitor",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
            disable_mcp: true,
            yolo: false,
            ..Default::default()
        })
    }

    fn validate_execution_policy(
        &self,
        _key: &str,
        policy: &pontia_core::execution_policy::ExecutionPolicy,
    ) -> Result<()> {
        if policy
            .tools
            .iter()
            .flatten()
            .chain(&policy.disabled_tools)
            .any(|tool| tool.trim().is_empty())
        {
            return Err(pontia_core::Error::Domain(
                "Claude tool entries must be nonempty native names or rules".into(),
            ));
        }
        Ok(())
    }

    fn supports_startup_model(&self) -> bool {
        true
    }

    fn supports_startup_reasoning_effort(&self) -> bool {
        true
    }

    fn list_models<'a>(
        &'a self,
        _: &'a Value,
        directory: &'a Path,
    ) -> ClientOperation<'a, pontia_agent_control::agent_clients::AgentClientModels> {
        Box::pin(async move {
            let script = std::path::absolute(
                self.host
                    .clone()
                    .unwrap_or_else(|| setup::host_path(directory)),
            )?;
            models::list(&script, directory).await
        })
    }

    fn detect(&self, _: &Value, context: &DetectionContext<'_>) -> ClientDetection {
        setup::detect(context, self.host.as_deref())
    }
    fn keys(&self) -> Vec<String> {
        vec!["default".into()]
    }
    fn presentation(&self, _: &str, _: &Value) -> (String, String) {
        ("Claude Code".into(), "claude-agent".into())
    }
    fn key(&self, _: &Value) -> Result<String> {
        Ok("default".into())
    }
    fn merged(&self, _: &str) -> Value {
        json!({})
    }
    fn resolve(&self, key: &str) -> Result<Value> {
        if key != "default" {
            return Err(unsupported());
        }
        Ok(json!({}))
    }
}
#[derive(Clone)]
struct Adapter {
    host: Option<PathBuf>,
    interactions: Arc<OnceLock<pontia_agent_control::interactions::InteractionService>>,
    peers: Arc<Mutex<HashMap<String, Arc<Peer>>>>,
    stopped: Arc<AtomicBool>,
    context: Arc<OnceLock<(SqlitePool, PathBuf)>>,
}
fn unsupported() -> Error {
    Error::CapabilityUnavailable("This Claude Code capability is not implemented".into())
}
fn unavailable() -> Error {
    Error::CapabilityUnavailable("Claude Code host is unavailable".into())
}
impl Adapter {
    fn script(&self, root: &Path) -> Result<PathBuf> {
        let script =
            std::path::absolute(self.host.clone().unwrap_or_else(|| setup::host_path(root)))?;
        if !script.is_file() {
            return Err(Error::CapabilityUnavailable(
                "Claude Code client is not installed; run pontia init and select claude-agent"
                    .into(),
            ));
        }
        Ok(script)
    }
    async fn peer(&self, session: &str) -> Result<Arc<Peer>> {
        self.peers
            .lock()
            .await
            .get(session)
            .cloned()
            .ok_or_else(unavailable)
    }
    async fn target(
        &self,
        events: &EventIngestService,
        target: &ControlTarget,
    ) -> Result<Arc<Peer>> {
        target.validate(&events.db()).await?;
        let peer = self.peer(&target.session_id).await?;
        if Some(peer.runtime.as_str()) != target.runtime_id.as_deref()
            || self.stopped.load(Ordering::Acquire)
        {
            return Err(unavailable());
        }
        Ok(peer)
    }
}
impl ClientSession for Adapter {
    fn provision<'a>(
        &'a self,
        events: EventIngestService,
        root: &'a Path,
        request: RuntimeStartRequest,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let mut peers = self.peers.lock().await;
            if self.stopped.load(Ordering::Acquire) || peers.contains_key(&request.session_id) {
                return Err(unavailable());
            }
            let script = self.script(root)?;
            let cwd = request
                .workspace
                .map(PathBuf::from)
                .ok_or_else(|| Error::Domain("Claude Code requires a Workspace".into()))?;
            let runtime = request.runtime_id.ok_or_else(unavailable)?;
            let config_dir = request
                .environment
                .get("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude"))
                })
                .ok_or_else(unavailable)?;
            let config_dir = std::path::absolute(cwd.join(config_dir))?;
            let peer = Arc::new(Peer {
                session: request.session_id,
                runtime,
                cwd,
                config_dir,
                model: request.model,
                resume: None,
                interactions: self.interactions.get().cloned().ok_or_else(unavailable)?,
                state: Default::default(),
                connection: Default::default(),
            });
            peers.insert(peer.session.clone(), peer.clone());
            drop(peers);
            peer.start(
                &events,
                root,
                &script,
                &request.environment,
                request.reasoning_effort.as_deref(),
            )
            .await?;
            Ok(())
        })
    }
    fn input<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        text: &'a str,
        turn: Option<&'a str>,
        message: Option<&'a str>,
        intent: &'a InputIntent,
    ) -> ClientOperation<'a, InputReceipt> {
        Box::pin(async move {
            let peer = self.target(&events, target).await?;
            if let InputIntent::Steer { turn_id } = intent {
                let target_input = {
                    let state = peer.state.lock().await;
                    if state.unavailable.is_some() || state.closing || state.closed {
                        return Err(unavailable());
                    }
                    let input = state.input.as_ref().ok_or_else(unavailable)?;
                    if input.turn.as_deref() != Some(turn_id.as_str()) {
                        return Err(Error::Conflict {
                            code: "steer_target_changed",
                            message: "Claude Code steering target is no longer active".into(),
                        });
                    }
                    input.id.clone()
                };
                let result = peer
                    .connection()?
                    .request(json!({
                        "method": "steer", "target_input_id": target_input,
                        "input_id": uuid::Uuid::new_v4().to_string(), "text": text,
                    }))
                    .await;
                if let Err(error) = result {
                    if matches!(error, Error::ControlUnknown(_)) {
                        peer.state.lock().await.unavailable = Some("input_delivery_unknown".into());
                    }
                    return Err(error);
                }
                return Ok(InputReceipt {
                    turn_pending: false,
                    native_turn_id: None,
                    runtime_id: Some(peer.runtime.clone()),
                });
            }
            if turn.is_some() {
                return Err(unsupported());
            }
            let id = uuid::Uuid::new_v4().to_string();
            {
                let mut state = peer.state.lock().await;
                if state.unavailable.is_some() || state.closing || state.closed {
                    return Err(unavailable());
                }
                if state.input.is_some() {
                    return Err(Error::Conflict {
                        code: "input_busy",
                        message: "Claude Code input is still active".into(),
                    });
                }
                state.input = Some(Input {
                    id: id.clone(),
                    turn: None,
                    sequence: 0,
                });
            }
            let result = peer
                .connection()?
                .request(json!({"method":"input","input_id":id,"text":text,"input":{"summary":text.chars().take(pontia_core::domain::MAX_TURN_INPUT_SUMMARY_CHARS).collect::<String>(),"inbox_message_id":message}}))
                .await;
            if let Err(error) = result {
                let mut state = peer.state.lock().await;
                if matches!(error, Error::ControlUnknown(_)) {
                    state.unavailable = Some("input_delivery_unknown".into());
                } else if state
                    .input
                    .as_ref()
                    .is_some_and(|input| input.id == id && input.turn.is_none())
                {
                    state.input = None;
                }
                return Err(error);
            }
            Ok(InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: Some(peer.runtime.clone()),
            })
        })
    }
    fn compact<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        operation_id: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let peer = self.target(&events, target).await?;
            peer.connection()?
                .request(json!({"method":"compact", "operation_id":operation_id}))
                .await
                .map(|_| ())
        })
    }
    fn manual_compaction_available<'a>(
        &'a self,
        _: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Ok(peer) = self.peer(session).await else {
                return Ok(false);
            };
            let state = peer.state.lock().await;
            Ok(state.native.is_some()
                && state.unavailable.is_none()
                && !state.closed
                && !state.closing
                && !self.stopped.load(Ordering::Acquire))
        })
    }
    fn interrupt<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        turn: Option<&'a str>,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let peer = self.target(&events, target).await?;
            let id = {
                let state = peer.state.lock().await;
                let input = state.input.as_ref().ok_or_else(unavailable)?;
                if turn.is_none() || input.turn.as_deref() != turn || state.closing || state.closed
                {
                    return Err(unavailable());
                }
                input.id.clone()
            };
            peer.connection()?
                .request(json!({"method":"interrupt","input_id":id}))
                .await
                .map(|_| ())
        })
    }
    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
        mode: ClientExitMode,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move {
            let peer = self.target(&events, target).await?;
            let connection = peer.connection()?;
            if mode == ClientExitMode::IfIdle {
                let response = connection.request(json!({"method":"is_idle"})).await?;
                match response["idle"].as_bool() {
                    Some(false) => return Ok(ClientExitOutcome::Busy),
                    Some(true) => {}
                    None => {
                        return Err(Error::Domain("Claude Code host omitted idle status".into()));
                    }
                }
            }
            peer.state.lock().await.closing = true;
            connection.request(json!({"method":"close"})).await?;
            connection.wait_exit().await?;
            if !peer.state.lock().await.closed {
                return Err(Error::ControlUnknown(
                    "Claude Code execution closure not confirmed".into(),
                ));
            }
            Ok(ClientExitOutcome::Confirmed {
                reason: "claude_sdk_closed".into(),
            })
        })
    }
    fn capabilities<'a>(
        &'a self,
        session: &'a str,
        implementation: &'a AgentClientCapabilities,
    ) -> ClientOperation<'a, AgentClientCapabilities> {
        Box::pin(async move {
            let mut offline = AgentClientCapabilities {
                timeline: true,
                compaction_events: implementation.compaction_events,
                ..Default::default()
            };
            if let Some((pool, _)) = self.context.get() {
                offline.accept_task = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM sessions s WHERE s.session_id=? AND s.state='exited' AND json_extract((SELECT payload FROM events WHERE session_id=s.session_id AND event_type='session.exited' ORDER BY rowid DESC LIMIT 1),'$.reason')='claude_sdk_closed')")
                    .bind(session).fetch_one(pool).await?;
            }
            let Ok(peer) = self.peer(session).await else {
                return Ok(offline);
            };
            let state = peer.state.lock().await;
            if state.unavailable.is_some() || state.closed || state.closing {
                return Ok(offline);
            }
            Ok(implementation.clone())
        })
    }
    fn available<'a>(&'a self, _: SqlitePool, session: &'a str) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Ok(peer) = self.peer(session).await else {
                return Ok(false);
            };
            Ok(!self.stopped.load(Ordering::Acquire) && !peer.connection()?.rpc.is_closed())
        })
    }
    fn details<'a>(
        &'a self,
        _: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async move {
            let reason = match self.peer(session).await {
                Ok(peer) => {
                    let state = peer.state.lock().await;
                    state
                        .unavailable
                        .clone()
                        .or_else(|| state.retry_reason.clone())
                }
                Err(_) => Some("host_unavailable".into()),
            };
            Ok(ClientSessionDetails {
                data: json!({"unavailable_reason":reason}),
            })
        })
    }
    fn resume<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move { self.resume_bound(events, target).await })
    }
}
impl ClientInteractions for Adapter {
    fn resolve<'a>(
        &'a self,
        _events: EventIngestService,
        session: &'a str,
        interaction: &'a str,
        option: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            if self.stopped.load(Ordering::Acquire) {
                return Err(unavailable());
            }
            let peer = self.peer(session).await?;
            interactions::resolve(&peer, interaction, option).await
        })
    }
}
impl ClientService for Adapter {
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
        Box::pin(self.serve_connection(state, process_id, rpc, first, requests))
    }

    fn phase(&self) -> ClientServicePhase {
        ClientServicePhase::Transport
    }
    fn start<'a>(
        &'a self,
        app: AgentControl,
    ) -> ClientOperation<'a, Option<tokio::task::JoinHandle<Result<()>>>> {
        let adapter = self.clone();
        Box::pin(async move {
            let _ = self.interactions.set(app.interactions());
            let _ = self.context.set((app.db(), app.pontia_home().into()));
            let mut shutdown = app.shutdown().subscribe();
            Ok(Some(tokio::spawn(async move {
                let _ = shutdown.wait_for(|stop| *stop).await;
                adapter.stopped.store(true, Ordering::Release);
                let peers: Vec<_> = adapter
                    .peers
                    .lock()
                    .await
                    .drain()
                    .map(|(_, peer)| peer)
                    .collect();
                for peer in &peers {
                    if let Ok(connection) = peer.connection() {
                        connection.stop();
                    }
                }
                Ok(())
            })))
        })
    }
}

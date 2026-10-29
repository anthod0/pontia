mod output;
mod peer;
mod spec;
mod transport;

use peer::{Input, Peer};
use pontia_application::{
    AppState, EventIngestService, PontiaEventType, client_contract::*, clients::ClientRegistration,
    control::InputReceipt, runtime::ControlTarget, sessions::SessionModel, turns::InputIntent,
};
use pontia_core::{Error, Result};
use pontia_runtime::{RuntimeStartRequest, RuntimeStartResult};
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
        peers: Default::default(),
        stopped: Arc::new(AtomicBool::new(false)),
    });
    ClientRegistration {
        configuration: Some(Arc::new(Configuration)),
        interactions: None,
        events: None,
        service: Some(adapter.clone()),
        profile: None,
        in_process: None,
        session: Some(adapter),
        prepare_on_input: true,
        steer: false,
        spec: &spec::SPEC,
        data: None,
        launcher: None,
    }
}

struct Configuration;
impl ClientConfiguration for Configuration {
    fn keys(&self) -> Vec<String> {
        vec!["default".into()]
    }
    fn presentation(&self, _: &str, _: &Value) -> (String, String) {
        ("Claude Agent".into(), "claude-agent".into())
    }
    fn key(&self, _: &Value) -> Result<String> {
        Ok("default".into())
    }
    fn merged(&self, _: &str, _: &Value) -> Value {
        json!({})
    }
    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value> {
        if key != "default" || overrides.as_object().is_none_or(|map| !map.is_empty()) {
            return Err(unsupported());
        }
        Ok(json!({}))
    }
}
#[derive(Clone)]
struct Adapter {
    host: Option<PathBuf>,
    peers: Arc<Mutex<HashMap<String, Arc<Peer>>>>,
    stopped: Arc<AtomicBool>,
}
fn unsupported() -> Error {
    Error::CapabilityUnavailable("This Claude Agent capability is not implemented".into())
}
fn unavailable() -> Error {
    Error::CapabilityUnavailable("Claude Agent host is unavailable".into())
}
impl Adapter {
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
    fn creation_options(&self) -> ClientOperation<'_, Option<Vec<SessionCreationOption>>> {
        Box::pin(async {
            Ok(Some(vec![SessionCreationOption {
                id: "claude-agent".into(),
                name: "Claude Agent".into(),
                client_type: "claude-agent".into(),
                metadata: json!({}),
            }]))
        })
    }
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
            let script = std::path::absolute(
                self.host
                    .clone()
                    .unwrap_or_else(|| root.join("clients/claude/dist/src/main.js")),
            )?;
            if !script.is_file() {
                return Err(Error::CapabilityUnavailable("Claude Agent SDK host is not installed; set PONTIA_CLAUDE_HOST_PATH to its built main.js".into()));
            }
            let cwd = request
                .workspace
                .map(PathBuf::from)
                .ok_or_else(|| Error::Domain("Claude Agent requires a Workspace".into()))?;
            let runtime = request.runtime_id.ok_or_else(unavailable)?;
            let directory = std::path::absolute(root)?
                .join("state/claude")
                .join(&runtime);
            // Build the private leaf atomically; never reuse a prior host's socket directory.
            std::fs::create_dir_all(directory.parent().ok_or_else(unavailable)?)?;
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().mode(0o700).create(&directory)?;
            let config_dir = request
                .environment
                .get("CLAUDE_CONFIG_DIR")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from))
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".claude"))
                })
                .ok_or_else(unavailable)?;
            let peer = Arc::new(Peer {
                session: request.session_id,
                runtime,
                cwd,
                config_dir,
                state: Default::default(),
                connection: OnceLock::new(),
            });
            peer.runtime_event(&events, PontiaEventType::RuntimeStarting, json!({}))
                .await?;
            match transport::Connection::spawn(
                peer.clone(),
                &script,
                &directory.join("control.sock"),
                &request.environment,
                events.clone(),
            )
            .await
            {
                Ok(connection) => {
                    let _ = peer.connection.set(connection);
                }
                Err(error) => {
                    peer.runtime_event(
                        &events,
                        PontiaEventType::RuntimeExited,
                        json!({"reason":"host_start_failed"}),
                    )
                    .await?;
                    return Err(error);
                }
            }
            peers.insert(peer.session.clone(), peer);
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
            if !matches!(intent, InputIntent::Start) || turn.is_some() {
                return Err(unsupported());
            }
            let peer = self.target(&events, target).await?;
            let id = uuid::Uuid::new_v4().to_string();
            {
                let mut state = peer.state.lock().await;
                if state.unavailable.is_some() || state.closing || state.closed {
                    return Err(unavailable());
                }
                if state.input.is_some() {
                    return Err(Error::Conflict {
                        code: "input_busy",
                        message: "Claude Agent input is still active".into(),
                    });
                }
                state.input = Some(Input {
                    id: id.clone(),
                    inbox: message.map(str::to_owned),
                    summary: text
                        .chars()
                        .take(pontia_core::domain::MAX_TURN_INPUT_SUMMARY_CHARS)
                        .collect(),
                    turn: None,
                    sequence: 0,
                });
            }
            let result = peer
                .connection()?
                .request(json!({"method":"input","input_id":id,"text":text}))
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
                native_turn_id: None,
                runtime_id: Some(peer.runtime.clone()),
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
        })
    }
    fn exit<'a>(
        &'a self,
        events: EventIngestService,
        target: &'a ControlTarget,
    ) -> ClientOperation<'a, ClientExitOutcome> {
        Box::pin(async move {
            let peer = self.target(&events, target).await?;
            peer.state.lock().await.closing = true;
            peer.connection()?
                .request(json!({"method":"close"}))
                .await?;
            peer.connection()?.wait_exit().await?;
            if !peer.state.lock().await.closed {
                return Err(Error::ControlUnknown(
                    "Claude Agent SDK close not confirmed".into(),
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
            let Ok(peer) = self.peer(session).await else {
                return Ok(AgentClientCapabilities::default());
            };
            let state = peer.state.lock().await;
            if state.unavailable.is_some() || state.closed || state.closing {
                return Ok(AgentClientCapabilities::default());
            }
            Ok(implementation.clone())
        })
    }
    fn available<'a>(&'a self, _: SqlitePool, session: &'a str) -> ClientOperation<'a, bool> {
        Box::pin(async move {
            let Ok(peer) = self.peer(session).await else {
                return Ok(false);
            };
            Ok(!self.stopped.load(Ordering::Acquire) && !*peer.connection()?.exited.borrow())
        })
    }
    fn details<'a>(
        &'a self,
        _: SqlitePool,
        session: &'a str,
    ) -> ClientOperation<'a, ClientSessionDetails> {
        Box::pin(async move {
            let reason = match self.peer(session).await {
                Ok(peer) => peer.state.lock().await.unavailable.clone(),
                Err(_) => Some("host_unavailable".into()),
            };
            Ok(ClientSessionDetails {
                data: json!({"unavailable_reason":reason}),
                model_control_unavailable_reason: Some(
                    "Claude Agent model control is not implemented".into(),
                ),
            })
        })
    }
    fn resume<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async { Err(unsupported()) })
    }
    fn list_models<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
    ) -> ClientOperation<'a, Vec<SessionModel>> {
        Box::pin(async { Err(unsupported()) })
    }
    fn set_model<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a ControlTarget,
        _: &'a str,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async { Err(unsupported()) })
    }
    fn open_interface<'a>(
        &'a self,
        _: EventIngestService,
        _: &'a Path,
        _: &'a str,
        _: &'a str,
    ) -> ClientOperation<'a, RuntimeStartResult> {
        Box::pin(async { Err(unsupported()) })
    }
}
impl ClientService for Adapter {
    fn phase(&self) -> ClientServicePhase {
        ClientServicePhase::Transport
    }
    fn start<'a>(
        &'a self,
        app: AppState,
    ) -> ClientOperation<'a, Option<tokio::task::JoinHandle<Result<()>>>> {
        let adapter = self.clone();
        Box::pin(async move {
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
                for peer in peers {
                    if let Ok(connection) = peer.connection() {
                        connection.wait_exit().await?;
                    }
                }
                Ok(())
            })))
        })
    }
}

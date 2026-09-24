use super::{CodexRuntime, TuiTarget, protocol};
use futures_util::{SinkExt, StreamExt};
use pontia_core::Result;
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use tokio::net::{UnixListener, UnixStream};
use tokio::{
    sync::{Mutex, oneshot},
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::tungstenite::Message;

pub(super) struct Gateway {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
    path: PathBuf,
    pub(super) state: Arc<Mutex<GatewayState>>,
}

#[derive(Default)]
pub(super) struct GatewayState {
    connections: HashMap<String, String>,
    current: Option<String>,
    retired: HashSet<String>,
    pub(super) quiet_since: Option<tokio::time::Instant>,
}

impl Gateway {
    pub async fn close(self) {
        let _ = self.stop.send(());
        let _ = self.task.await;
        let _ = std::fs::remove_file(self.path);
    }
}

impl CodexRuntime {
    pub(super) async fn gateway(self: &Arc<Self>, owner: &str) -> Result<String> {
        let _current = self.current_guard().await?;
        let mut gateways = self.gateways.lock().await;
        let path = self
            .root
            .join("state/codex")
            .join(format!("tui-{}.sock", owner.trim_start_matches("sess_")));
        let endpoint = format!("unix://{}", path.display());
        if gateways.contains_key(owner) {
            return Ok(endpoint);
        }
        use std::os::unix::fs::PermissionsExt;
        let directory = self.root.join("state/codex");
        std::fs::create_dir_all(&directory)?;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        let listener = UnixListener::bind(&path)?;
        let runtime = self.clone();
        let owner_id = owner.to_string();
        let (stop, mut stopping) = oneshot::channel();
        let state = Arc::new(Mutex::new(GatewayState {
            quiet_since: Some(tokio::time::Instant::now()),
            ..Default::default()
        }));
        let connection_state = state.clone();
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    _ = &mut stopping => break,
                    _ = connections.join_next(), if !connections.is_empty() => {},
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break };
                        let runtime = runtime.clone();
                        let owner = owner_id.clone();
                        let state = connection_state.clone();
                        connections.spawn(async move {
                            let identity = match super::daemon::DaemonIdentity::capture(&stream) {
                                Ok(identity) => identity.instance_id,
                                Err(_) => return,
                            };
                            let id = pontia_core::ids::new_runtime_instance_id().to_string();
                            {
                                let mut state = state.lock().await;
                                if state.retired.contains(&identity) || state.connections.values().any(|peer| peer != &identity) { return; }
                                if let Ok(Some(process)) = runtime.saved_tui::<super::tui::TuiProcess>(&owner, "process")
                                    && process.is_alive() && process.peer_identity() != identity { return; }
                                let mut targets = runtime.tui_targets.lock().await;
                                let thread = match runtime.saved_target(&owner) {
                                    Ok(target) => target.filter(|target| target.peer_identity == identity).map(|target| target.thread).unwrap_or(Value::Null),
                                    Err(error) => { tracing::warn!(%owner, %error, "cannot read TUI target"); return; }
                                };
                                let target = TuiTarget { connection_id: id.clone(), peer_identity: identity.clone(), owner_session_id: owner.clone(), thread, connected: false, error: None };
                                if let Err(error) = runtime.save_tui(&owner, "target", &target) {
                                    tracing::warn!(%owner, %error, "cannot save TUI connection"); return;
                                }
                                state.connections.insert(id.clone(), identity.clone());
                                state.current = Some(id.clone());
                                targets.insert(owner.clone(), target.clone());
                                let _ = runtime.targets.send(target);
                            }
                            if let Err(error) = forward(stream, runtime.clone(), owner.clone(), id.clone(), identity).await {
                                tracing::warn!(%owner, %error, "Codex TUI gateway connection failed");
                            }
                            let target = runtime.tui_targets.lock().await.get(&owner).filter(|target| target.connection_id == id).cloned();
                            if let Some(mut target) = target {
                                target.connected = false;
                                let _ = runtime.record_target(target).await;
                            }
                            let mut state = state.lock().await;
                            state.connections.remove(&id);
                            if state.connections.is_empty() { state.quiet_since = Some(tokio::time::Instant::now()); }
                        });
                    }
                }
            }
            connections.shutdown().await;
        });
        gateways.insert(
            owner.to_string(),
            Gateway {
                stop,
                task,
                path,
                state,
            },
        );
        Ok(endpoint)
    }
    pub(super) async fn retire_tui_connection(&self, owner: &str, peer: &str) -> Result<()> {
        let gateways = self.gateways.lock().await;
        let gateway = gateways
            .get(owner)
            .ok_or_else(|| protocol::protocol_error("TUI gateway is missing"))?;
        let mut state = gateway.state.lock().await;
        if !state.connections.is_empty() {
            return Err(protocol::protocol_error(
                "TUI connection is still active or restoring; retry Open TUI after native recovery",
            ));
        }
        let elapsed = state
            .quiet_since
            .map(|since| since.elapsed())
            .unwrap_or_default();
        if elapsed < super::tui::RECONNECT_WINDOW {
            return Err(protocol::protocol_error(format!(
                "TUI native recovery is still pending; retry Open TUI in {} seconds",
                (super::tui::RECONNECT_WINDOW - elapsed).as_secs() + 1
            )));
        }
        state.retired.insert(peer.into());
        Ok(())
    }

    async fn record_target(&self, target: TuiTarget) -> Result<()> {
        let gateways = self.gateways.lock().await;
        let Some(gateway) = gateways.get(&target.owner_session_id) else {
            return Ok(());
        };
        let state = gateway.state.lock().await;
        if state.current.as_deref() != Some(&target.connection_id) {
            return Ok(());
        }
        let mut targets = self.tui_targets.lock().await;
        self.save_tui(&target.owner_session_id, "target", &target)?;
        targets.insert(target.owner_session_id.clone(), target.clone());
        let _ = self.targets.send(target);
        Ok(())
    }
}

async fn forward(
    stream: UnixStream,
    runtime: Arc<CodexRuntime>,
    owner: String,
    connection_id: String,
    peer_identity: String,
) -> Result<()> {
    let mut client = tokio::time::timeout(
        Duration::from_secs(5),
        tokio_tungstenite::accept_async(stream),
    )
    .await
    .map_err(protocol::protocol_error)?
    .map_err(protocol::protocol_error)?;
    let mut server = protocol::open(&runtime.socket_path).await?;
    if super::daemon::DaemonIdentity::capture(server.get_ref())? != runtime.connection.identity {
        return Err(protocol::protocol_error(
            "daemon changed before TUI connection",
        ));
    }
    let mut pending = HashMap::<String, bool>::new();
    let mut attached = false;
    let mut target = runtime
        .saved_target(&owner)?
        .filter(|target| target.peer_identity == peer_identity)
        .map(|target| target.thread)
        .unwrap_or(Value::Null);
    loop {
        tokio::select! {
            frame = client.next() => {
                let Some(Ok(mut frame)) = frame else { break };
                if let Message::Text(text) = &frame
                    && let Ok(mut request) = serde_json::from_str::<Value>(text)
                    && request["method"] == "thread/start"
                    && request["params"]["historyMode"].is_null() {
                    // 0.156.1 paginated history cannot resume or provide Turn snapshots.
                    request["params"]["historyMode"] = serde_json::json!("legacy");
                    frame = Message::Text(request.to_string().into());
                }
                if let Message::Text(text) = &frame
                    && let Ok(value) = serde_json::from_str::<Value>(text)
                    && matches!(value["method"].as_str(), Some("thread/start" | "thread/resume" | "thread/fork"))
                    && value.pointer("/params/ephemeral").and_then(Value::as_bool) != Some(true)
                    && !matches!(value.pointer("/params/threadSource").and_then(Value::as_str), Some("system")) {
                    let switching = value["method"] != "thread/resume" || value["params"]["threadId"] != target["id"];
                    pending.insert(value["id"].to_string(), switching);
                    let confirmed = if pending.values().any(|switching| *switching) { Value::Null } else { target.clone() };
                    runtime.record_target(TuiTarget { connection_id: connection_id.clone(), peer_identity: peer_identity.clone(), owner_session_id: owner.clone(), thread: confirmed, connected: false, error: None }).await?;
                }
                if server.send(frame).await.is_err() { break; }
            }
            frame = server.next() => {
                let Some(Ok(frame)) = frame else { break };
                if let Message::Text(text) = &frame
                    && let Ok(value) = serde_json::from_str::<Value>(text)
                    && value.get("method").is_none()
                    && pending.remove(&value["id"].to_string()).is_some() {
                    if let Some(thread) = value.pointer("/result/thread").filter(|thread| thread["id"].is_string()) {
                        target = serde_json::json!({"id":thread["id"],"cwd":thread["cwd"],"path":thread["path"],"status":thread["status"]});
                        attached = true;
                    }
                    let confirmed = if pending.values().any(|switching| *switching) { Value::Null } else { target.clone() };
                    runtime.record_target(TuiTarget { connection_id: connection_id.clone(), peer_identity: peer_identity.clone(), owner_session_id: owner.clone(), thread: confirmed, connected: attached && pending.is_empty(), error: value.get("error").map(|error| format!("Codex TUI attachment rejected: {error}")) }).await?;
                }
                if client.send(frame).await.is_err() { break; }
            }
        }
    }
    let error = runtime
        .saved_target(&owner)?
        .and_then(|target| target.error);
    runtime
        .record_target(TuiTarget {
            connection_id,
            peer_identity,
            owner_session_id: owner,
            thread: if pending.values().any(|switching| *switching) {
                Value::Null
            } else {
                target
            },
            connected: false,
            error,
        })
        .await?;
    let _ = server.close(None).await;
    let _ = client.close(None).await;
    Ok(())
}

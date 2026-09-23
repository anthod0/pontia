use super::{CodexRuntime, TuiTarget, protocol};
use futures_util::{SinkExt, StreamExt};
use pontia_core::Result;
use serde_json::Value;
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tokio::net::{UnixListener, UnixStream};
use tokio::{
    sync::oneshot,
    task::{JoinHandle, JoinSet},
};
use tokio_tungstenite::tungstenite::Message;

pub(super) struct Gateway {
    stop: oneshot::Sender<()>,
    task: JoinHandle<()>,
    path: PathBuf,
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
                        connections.spawn(async move {
                            let _ = forward(stream, runtime, owner).await;
                        });
                    }
                }
            }
            connections.shutdown().await;
        });
        gateways.insert(owner.to_string(), Gateway { stop, task, path });
        Ok(endpoint)
    }
}

async fn forward(stream: UnixStream, runtime: Arc<CodexRuntime>, owner: String) -> Result<()> {
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
    let mut pending = HashMap::<String, String>::new();
    let mut target = Value::Null;
    let connection_id = pontia_core::ids::new_runtime_instance_id().to_string();
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
                    pending.insert(value["id"].to_string(), value["method"].as_str().unwrap().to_string());
                }
                if server.send(frame).await.is_err() { break; }
            }
            frame = server.next() => {
                let Some(Ok(frame)) = frame else { break };
                if let Message::Text(text) = &frame
                    && let Ok(value) = serde_json::from_str::<Value>(text)
                    && value.get("method").is_none()
                    && pending.remove(&value["id"].to_string()).is_some()
                    && value.pointer("/result/thread/id").and_then(Value::as_str).is_some() {
                    let thread = &value["result"]["thread"];
                    target = serde_json::json!({"id":thread["id"],"cwd":thread["cwd"],"path":thread["path"],"status":thread["status"]});
                    runtime.record_target(TuiTarget { connection_id:connection_id.clone(), owner_session_id: owner.clone(), thread: target.clone(), connected: true }).await;
                }
                if client.send(frame).await.is_err() { break; }
            }
        }
    }
    runtime
        .record_target(TuiTarget {
            connection_id,
            owner_session_id: owner,
            thread: target,
            connected: false,
        })
        .await;
    let _ = server.close(None).await;
    let _ = client.close(None).await;
    Ok(())
}

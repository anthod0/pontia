use super::{CodexRuntime, TuiTarget, protocol};
use futures_util::{SinkExt, StreamExt};
use pontia_core::Result;
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};
use tokio::net::{UnixListener, UnixStream};
use tokio_tungstenite::tungstenite::Message;

impl CodexRuntime {
    pub(super) async fn gateway(self: &Arc<Self>, owner: &str) -> Result<String> {
        let mut gateways = self.gateways.lock().await;
        let path = self
            .root
            .join("state/codex")
            .join(format!("tui-{}.sock", owner.trim_start_matches("sess_")));
        let endpoint = format!("unix://{}", path.display());
        if gateways.contains_key(owner) {
            return Ok(endpoint);
        }
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        let listener = UnixListener::bind(path)?;
        let runtime = self.clone();
        let owner_id = owner.to_string();
        let task = tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let runtime = runtime.clone();
                let owner = owner_id.clone();
                tokio::spawn(async move {
                    let _ = forward(stream, runtime, owner).await;
                });
            }
        });
        gateways.insert(owner.to_string(), task);
        Ok(endpoint)
    }
}

async fn forward(stream: UnixStream, runtime: Arc<CodexRuntime>, owner: String) -> Result<()> {
    let mut client = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(protocol::protocol_error)?;
    let mut server = protocol::open(&runtime.socket_path).await?;
    let mut pending = HashMap::<String, String>::new();
    let mut target = Value::Null;
    let connection_id = pontia_core::ids::new_runtime_instance_id().to_string();
    loop {
        tokio::select! {
            frame = client.next() => {
                let Some(Ok(frame)) = frame else { break };
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

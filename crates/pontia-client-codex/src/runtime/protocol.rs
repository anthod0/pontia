use super::daemon::{DaemonIdentity, SUPPORTED_VERSION};
use futures_util::{SinkExt, StreamExt};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    net::UnixStream,
    sync::{Mutex, broadcast, mpsc, oneshot},
};
use tokio_tungstenite::{WebSocketStream, tungstenite::Message};

pub type Socket = WebSocketStream<UnixStream>;
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value>>>>>;

pub async fn open(path: &Path) -> Result<Socket> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let stream = UnixStream::connect(path).await.map_err(|error| {
            protocol_error(format!(
                "daemon at {} is unavailable: {error}; start Codex daemon externally",
                path.display()
            ))
        })?;
        let (socket, _) = tokio_tungstenite::client_async("ws://localhost/", stream)
            .await
            .map_err(protocol_error)?;
        Ok(socket)
    })
    .await
    .map_err(|_| protocol_error("daemon connection timed out"))?
}

pub fn protocol_error(error: impl std::fmt::Display) -> Error {
    Error::CapabilityUnavailable(format!("Codex control connection: {error}"))
}

pub struct Connection {
    outgoing: mpsc::Sender<Message>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub identity: DaemonIdentity,
    pub server_version: String,
    pub codex_home: PathBuf,
    pending: Pending,
    next_id: AtomicU64,
    pub events: broadcast::Sender<Value>,
}

impl Connection {
    pub async fn connect(path: &Path) -> Result<Arc<Self>> {
        let mut socket = open(path).await?;
        let identity = DaemonIdentity::capture(socket.get_ref())?;
        // Validate the peer's response before exposing a usable control connection.
        socket
            .send(Message::Text(
                json!({"id":0,"method":"initialize","params":{
                    "clientInfo":{"name":"pontia","version":env!("CARGO_PKG_VERSION")},
                    "capabilities":{"experimentalApi":true}
                }})
                .to_string()
                .into(),
            ))
            .await
            .map_err(protocol_error)?;
        let metadata = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match socket.next().await {
                    Some(Ok(Message::Text(text))) => {
                        let value: Value = serde_json::from_str(&text).map_err(protocol_error)?;
                        if value["id"] == 0 && value.get("method").is_none() {
                            if let Some(error) = value.get("error") {
                                return Err(protocol_error(error));
                            }
                            return Ok(value["result"].clone());
                        }
                    }
                    Some(Ok(Message::Ping(data))) => socket
                        .send(Message::Pong(data))
                        .await
                        .map_err(protocol_error)?,
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => {
                        return Err(protocol_error("daemon disconnected during initialization"));
                    }
                    _ => {}
                }
            }
        })
        .await
        .map_err(|_| protocol_error("daemon initialization timed out"))??;
        let version = metadata["userAgent"]
            .as_str()
            .and_then(|agent| agent.split_once('/'))
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .ok_or_else(|| protocol_error("daemon did not identify its version"))?;
        if version != SUPPORTED_VERSION {
            return Err(protocol_error(format!(
                "unsupported daemon version {version}; verified version is {SUPPORTED_VERSION}"
            )));
        }
        let codex_home = metadata["codexHome"]
            .as_str()
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| protocol_error("daemon did not identify its Codex home"))?;
        if metadata["platformOs"] != "linux" || metadata["platformFamily"] != "unix" {
            return Err(protocol_error("daemon must run locally on Linux"));
        }
        if DaemonIdentity::capture(socket.get_ref())? != identity {
            return Err(protocol_error("daemon changed during initialization"));
        }
        socket
            .send(Message::Text(
                json!({"method":"initialized"}).to_string().into(),
            ))
            .await
            .map_err(protocol_error)?;
        let (outgoing, mut incoming) = mpsc::channel(128);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (events, _) = broadcast::channel(4096);
        let connection = Arc::new(Self {
            outgoing,
            task: Mutex::new(None),
            identity,
            server_version: version.into(),
            codex_home,
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
            events: events.clone(),
        });
        let task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    message = incoming.recv() => {
                        let Some(message) = message else { break };
                        if socket.send(message).await.is_err() { break; }
                    }
                    frame = socket.next() => {
                        match frame {
                            Some(Ok(Message::Text(text))) => {
                                let Ok(value) = serde_json::from_str::<Value>(&text) else { break };
                                if value.get("method").is_some() {
                                    // Server requests belong to the official TUI. Observing never responds.
                                    let _ = events.send(value);
                                } else if let Some(id) = value["id"].as_u64()
                                    && let Some(reply) = pending.lock().await.remove(&id) {
                                    let result = if let Some(error) = value.get("error") {
                                        Err(Error::StateConflict(format!("Codex RPC: {error}")))
                                    } else { Ok(value["result"].clone()) };
                                    let _ = reply.send(result);
                                }
                            }
                            Some(Ok(Message::Ping(bytes))) => { if socket.send(Message::Pong(bytes)).await.is_err() { break; } }
                            Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                            _ => {}
                        }
                    }
                }
            }
            incoming.close();
            for (_, reply) in pending.lock().await.drain() {
                let _ = reply.send(Err(Error::ControlUnknown("Codex disconnected".into())));
            }
            let _ = events.send(json!({"method":"pontia/disconnected"}));
        });
        *connection.task.lock().await = Some(task);
        Ok(connection)
    }

    pub fn is_connected(&self) -> bool {
        !self.outgoing.is_closed()
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending.lock().await.insert(id, sender);
        let result = async {
            self.outgoing
                .send(Message::Text(
                    json!({"id":id,"method":method,"params":params})
                        .to_string()
                        .into(),
                ))
                .await
                .map_err(protocol_error)?;
            tokio::time::timeout(Duration::from_secs(30), receiver)
                .await
                .map_err(|_| Error::ControlUnknown("Codex RPC timed out".into()))?
                .map_err(|error| Error::ControlUnknown(error.to_string()))?
        }
        .await;
        self.pending.lock().await.remove(&id);
        result
    }

    pub async fn close(&self) {
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        for (_, reply) in self.pending.lock().await.drain() {
            let _ = reply.send(Err(Error::ControlUnknown("Codex connection closed".into())));
        }
        let _ = self.events.send(json!({"method":"pontia/disconnected"}));
    }
}

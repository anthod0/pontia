use super::daemon::DaemonIdentity;
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
enum RpcResponse {
    Success(Value, u64),
    Rejected(Value),
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub value: Value,
    pub sequence: u64,
}

impl std::ops::Deref for Notification {
    type Target = Value;

    fn deref(&self) -> &Value {
        &self.value
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<RpcResponse>>>>>;

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
    pub server_version: Option<String>,
    pub codex_home: PathBuf,
    pending: Pending,
    next_id: AtomicU64,
    pub events: broadcast::Sender<Notification>,
}

impl Connection {
    pub async fn connect(path: &Path) -> Result<Arc<Self>> {
        let mut socket = open(path).await?;
        let identity = DaemonIdentity::capture(socket.get_ref())?;
        // Validate the peer's response before exposing a usable control connection.
        socket
            .send(Message::Text(
                json!({"id":0,"method":"initialize","params":{
                    "clientInfo":{"name":"pontia","version":pontia_version::version()},
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
            .and_then(|(_, rest)| rest.split_whitespace().next());
        let codex_home = metadata["codexHome"]
            .as_str()
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| protocol_error("daemon did not identify its Codex home"))?;
        if metadata["platformOs"] != std::env::consts::OS || metadata["platformFamily"] != "unix" {
            return Err(protocol_error(format!(
                "daemon must run locally on {}",
                std::env::consts::OS
            )));
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
            server_version: version.map(str::to_owned),
            codex_home,
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
            events: events.clone(),
        });
        let task = tokio::spawn(async move {
            let mut notification_sequence = 0u64;
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
                                    notification_sequence += 1;
                                    let _ = events.send(Notification { value, sequence: notification_sequence });
                                } else if let Some(id) = value["id"].as_u64()
                                    && let Some(reply) = pending.lock().await.remove(&id) {
                                    let response = if let Some(error) = value.get("error") {
                                        RpcResponse::Rejected(error.clone())
                                    } else { RpcResponse::Success(value["result"].clone(), notification_sequence) };
                                    let _ = reply.send(Ok(response));
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
            let _ = events.send(Notification {
                value: json!({"method":"pontia/disconnected"}),
                sequence: notification_sequence + 1,
            });
        });
        *connection.task.lock().await = Some(task);
        Ok(connection)
    }

    pub fn is_connected(&self) -> bool {
        !self.outgoing.is_closed()
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        self.call_observed(method, params)
            .await
            .map(|(value, _)| value)
    }

    /// The boundary is captured by the socket reader, before waking the RPC caller.
    pub(crate) async fn call_observed(&self, method: &str, params: Value) -> Result<(Value, u64)> {
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
        match result? {
            RpcResponse::Success(value, sequence) => Ok((value, sequence)),
            RpcResponse::Rejected(error) => {
                // Codex reports this pre-materialization state without a dedicated error code.
                if error["code"] == -32600
                    && let Some(thread) = params["threadId"].as_str()
                {
                    let rejection = match method {
                        "thread/turns/list" => Some((
                            "codex_thread_not_materialized",
                            format!(
                                "thread {thread} is not materialized yet; thread/turns/list is unavailable before first user message"
                            ),
                        )),
                        "thread/resume" => Some((
                            "codex_thread_not_persisted",
                            format!("no rollout found for thread id {thread}"),
                        )),
                        _ => None,
                    };
                    if let Some((code, message)) = rejection
                        && error["message"].as_str() == Some(&message)
                    {
                        return Err(Error::Conflict { code, message });
                    }
                }
                Err(Error::StateConflict(format!("Codex RPC: {error}")))
            }
        }
    }

    pub async fn close(&self) {
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        for (_, reply) in self.pending.lock().await.drain() {
            let _ = reply.send(Err(Error::ControlUnknown("Codex connection closed".into())));
        }
        let _ = self.events.send(Notification {
            value: json!({"method":"pontia/disconnected"}),
            sequence: 0,
        });
    }
}

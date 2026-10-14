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
type ServerRequests = Arc<Mutex<HashMap<String, Notification>>>;

pub(crate) fn invalidates_request(
    event: &Value,
    id: &Value,
    thread: &str,
    turn: Option<&str>,
) -> bool {
    if event["params"]["threadId"].as_str() != Some(thread) {
        return false;
    }
    match event["method"].as_str() {
        Some("serverRequest/resolved") => event["params"]["requestId"] == *id,
        Some("turn/completed") => turn.is_none() || event["params"]["turn"]["id"].as_str() == turn,
        Some("thread/status/changed") => event["params"]["status"]["type"] == "notLoaded",
        _ => false,
    }
}

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

enum Outgoing {
    Message(Message),
    Reply {
        id: Value,
        result: Value,
        written: oneshot::Sender<Result<()>>,
    },
}

pub struct Connection {
    outgoing: mpsc::Sender<Outgoing>,
    pub(crate) interactions:
        Mutex<HashMap<String, crate::service::interactions::NativeInteraction>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pub identity: DaemonIdentity,
    pub server_version: Option<String>,
    pub codex_home: PathBuf,
    pending: Pending,
    server_requests: ServerRequests,
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
        let server_requests: ServerRequests = Arc::new(Mutex::new(HashMap::new()));
        let connection = Arc::new(Self {
            outgoing,
            interactions: Mutex::new(HashMap::new()),
            task: Mutex::new(None),
            identity,
            server_version: version.map(str::to_owned),
            codex_home,
            pending: pending.clone(),
            server_requests: server_requests.clone(),
            next_id: AtomicU64::new(1),
            events: events.clone(),
        });
        let task = tokio::spawn(async move {
            let mut notification_sequence = 0u64;
            loop {
                tokio::select! {
                    message = incoming.recv() => {
                        let Some(message) = message else { break };
                        match message {
                            Outgoing::Message(message) => { if socket.send(message).await.is_err() { break; } }
                            Outgoing::Reply { id, result, written } => {
                                if server_requests.lock().await.remove(&id.to_string()).is_none() {
                                    let _ = written.send(Err(protocol_error("server request is no longer pending")));
                                    continue;
                                }
                                let result = socket.send(Message::Text(json!({"id":id,"result":result}).to_string().into())).await.map_err(protocol_error);
                                let failed = result.is_err();
                                let _ = written.send(result);
                                if failed { break; }
                            }
                        }
                    }
                    frame = socket.next() => {
                        match frame {
                            Some(Ok(Message::Text(text))) => {
                                let Ok(value) = serde_json::from_str::<Value>(&text) else { break };
                                if value.get("method").is_some() {
                                    notification_sequence += 1;
                                    if let Some(id) = value.get("id").filter(|id| id.is_string() || id.is_number()) {
                                        server_requests.lock().await.insert(id.to_string(), Notification { value: value.clone(), sequence: notification_sequence });
                                    } else {
                                        server_requests.lock().await.retain(|_, request| {
                                            let params = &request["params"];
                                            let thread = params["threadId"].as_str().or(params["conversationId"].as_str()).unwrap_or("");
                                            !invalidates_request(&value, &request["id"], thread, params["turnId"].as_str())
                                        });
                                    }
                                    // Requests and notifications are observed without waiting for user input.
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
            server_requests.lock().await.clear();
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
                .send(Outgoing::Message(Message::Text(
                    json!({"id":id,"method":method,"params":params})
                        .to_string()
                        .into(),
                )))
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

    pub(crate) async fn pending_server_requests(&self) -> Vec<Notification> {
        let mut requests: Vec<_> = self
            .server_requests
            .lock()
            .await
            .values()
            .cloned()
            .collect();
        requests.sort_by_key(|request| request.sequence);
        requests
    }

    pub(crate) async fn reply(&self, id: Value, result: Value) -> Result<()> {
        let (written, receiver) = oneshot::channel();
        self.outgoing
            .send(Outgoing::Reply {
                id,
                result,
                written,
            })
            .await
            .map_err(protocol_error)?;
        receiver.await.map_err(protocol_error)?
    }

    pub async fn close(&self) {
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
            let _ = task.await;
        }
        self.server_requests.lock().await.clear();
        for (_, reply) in self.pending.lock().await.drain() {
            let _ = reply.send(Err(Error::ControlUnknown("Codex connection closed".into())));
        }
        let _ = self.events.send(Notification {
            value: json!({"method":"pontia/disconnected"}),
            sequence: 0,
        });
    }
}

use futures_util::{SinkExt, StreamExt};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::Path,
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
    let stream = UnixStream::connect(path).await?;
    let (socket, _) = tokio_tungstenite::client_async("ws://localhost/", stream)
        .await
        .map_err(protocol_error)?;
    Ok(socket)
}

pub fn protocol_error(error: impl std::fmt::Display) -> Error {
    Error::CapabilityUnavailable(format!("Codex control connection: {error}"))
}

pub struct Connection {
    outgoing: mpsc::Sender<Message>,
    pending: Pending,
    next_id: AtomicU64,
    pub events: broadcast::Sender<Value>,
}

impl Connection {
    pub async fn connect(path: &Path) -> Result<Arc<Self>> {
        let mut socket = open(path).await?;
        let (outgoing, mut incoming) = mpsc::channel(128);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (events, _) = broadcast::channel(4096);
        let connection = Arc::new(Self {
            outgoing,
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
            events: events.clone(),
        });
        tokio::spawn(async move {
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
        connection
            .call(
                "initialize",
                json!({"clientInfo":{"name":"pontia","version":env!("CARGO_PKG_VERSION")}}),
            )
            .await?;
        connection
            .outgoing
            .send(Message::Text(
                json!({"method":"initialized"}).to_string().into(),
            ))
            .await
            .map_err(protocol_error)?;
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
        let _ = self.outgoing.send(Message::Close(None)).await;
    }
}

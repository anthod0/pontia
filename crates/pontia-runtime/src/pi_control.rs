use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use pontia_core::{Error, Result};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixStream, unix::OwnedWriteHalf},
    sync::{mpsc, oneshot, watch},
};

pub const PROTOCOL_VERSION: u32 = 4;
pub const MAX_CONTROL_FRAME_BYTES: usize = 64 * 1024;
// The former HTTP event body limit plus space for the RPC envelope.
pub const MAX_FRAME_BYTES: usize = 2 * 1024 * 1024 + 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

pub struct RpcRequest {
    pub id: Option<Value>,
    pub method: String,
    pub params: Value,
}

/// The reader dispatches requests independently of outstanding calls in either direction.
pub struct PiRpcPeer {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    pending: Mutex<HashMap<String, oneshot::Sender<Value>>>,
    closed: watch::Sender<bool>,
    sequence: AtomicU64,
}

impl PiRpcPeer {
    pub fn new(stream: UnixStream) -> (Arc<Self>, mpsc::Receiver<RpcRequest>) {
        let (reader, writer) = stream.into_split();
        let (requests, incoming) = mpsc::channel(16);
        let peer = Arc::new(Self {
            writer: tokio::sync::Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            closed: watch::channel(false).0,
            sequence: AtomicU64::new(0),
        });
        let running = peer.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(reader);
            loop {
                let frame = tokio::select! {
                    biased;
                    _ = running.closed() => break,
                    frame = read_frame(&mut reader) => frame,
                };
                let frame = match frame {
                    Ok(frame) => frame,
                    Err(_) => break,
                };
                let value: Value = match serde_json::from_slice(&frame) {
                    Ok(value) => value,
                    Err(_) => {
                        let _ = running
                            .reply_error(Some(Value::Null), -32700, "Invalid JSON")
                            .await;
                        break;
                    }
                };
                if value["jsonrpc"] != "2.0" || !value.is_object() {
                    let _ = running
                        .reply_error(Some(Value::Null), -32600, "Invalid JSON-RPC request")
                        .await;
                    break;
                }
                if let Some(method) = value.get("method") {
                    let id = value.get("id").cloned();
                    if !method.is_string()
                        || id
                            .as_ref()
                            .is_some_and(|id| !id.is_string() && !id.is_number())
                        || value.get("result").is_some()
                        || value.get("error").is_some()
                    {
                        let _ = running
                            .reply_error(Some(Value::Null), -32600, "Invalid JSON-RPC request")
                            .await;
                        break;
                    }
                    let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
                    if !params.is_object() {
                        let _ = running
                            .reply_error(id, -32602, "Expected named parameters")
                            .await;
                        continue;
                    }
                    if requests
                        .try_send(RpcRequest {
                            id,
                            method: method.as_str().unwrap().into(),
                            params,
                        })
                        .is_err()
                    {
                        break;
                    }
                } else {
                    if value.get("result").is_some() == value.get("error").is_some()
                        || value.get("error").is_some_and(|error| {
                            !error["code"].is_i64() || !error["message"].is_string()
                        })
                    {
                        break;
                    }
                    let Some(id) = value["id"].as_str() else {
                        break;
                    };
                    let pending = running.pending.lock().unwrap().remove(id);
                    let Some(pending) = pending else { break };
                    let _ = pending.send(value);
                }
            }
            running.close();
            let _ = running.writer.lock().await.shutdown().await;
        });
        (peer, incoming)
    }

    pub fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    pub fn close(&self) {
        self.closed.send_replace(true);
        self.pending.lock().unwrap().clear();
    }

    pub async fn closed(&self) {
        let mut closed = self.closed.subscribe();
        let _ = closed.wait_for(|value| *value).await;
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        if self.is_closed() {
            return Err(Error::CapabilityUnavailable(
                "Pi connection is closed".into(),
            ));
        }
        let id = format!("pontia:{}", self.sequence.fetch_add(1, Ordering::Relaxed));
        let (send, receive) = oneshot::channel();
        let request = json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params});
        // Validate before creating an uncertain operation.
        encode(&request)?;
        self.pending.lock().unwrap().insert(id.clone(), send);
        let mut guard = PendingCall {
            peer: self,
            id,
            completed: false,
        };
        let result = tokio::time::timeout(REQUEST_TIMEOUT, async {
            self.write(request).await?;
            let response = receive.await.map_err(|_| {
                Error::ControlUnknown(
                    "Pi connection closed before response; request was not replayed".into(),
                )
            })?;
            if let Some(error) = response.get("error") {
                if error["code"] == -32007 {
                    return Err(Error::ControlUnknown(
                        error["message"].as_str().unwrap().into(),
                    ));
                }
                return Err(Error::Domain(format!(
                    "Pi RPC {}: {}",
                    error["code"], error["message"]
                )));
            }
            Ok(response["result"].clone())
        })
        .await;
        match result {
            Ok(result) => {
                guard.completed = true;
                result
            }
            Err(_) => Err(Error::ControlUnknown(
                "Pi RPC timed out; request was not replayed".into(),
            )),
        }
    }

    pub async fn reply(&self, id: Option<Value>, result: Value) -> Result<()> {
        if let Some(id) = id {
            self.write(json!({"jsonrpc":"2.0", "id":id, "result":result}))
                .await?;
        }
        Ok(())
    }

    pub async fn reply_error(&self, id: Option<Value>, code: i64, message: &str) -> Result<()> {
        if let Some(id) = id {
            self.write(json!({"jsonrpc":"2.0", "id":id, "error":{"code":code, "message":message}}))
                .await?;
        }
        Ok(())
    }

    async fn write(&self, value: Value) -> Result<()> {
        let encoded = encode(&value)?;
        let result = tokio::select! {
            biased;
            _ = self.closed() => return Err(Error::CapabilityUnavailable("Pi connection is closed".into())),
            result = tokio::time::timeout(REQUEST_TIMEOUT, async {
                self.writer.lock().await.write_all(&encoded).await
            }) => result,
        };
        match result {
            Ok(Ok(())) => Ok(()),
            _ => {
                self.close();
                Err(Error::ControlUnknown(
                    "Pi RPC write failed; request was not replayed".into(),
                ))
            }
        }
    }
}

struct PendingCall<'a> {
    peer: &'a PiRpcPeer,
    id: String,
    completed: bool,
}
impl Drop for PendingCall<'_> {
    fn drop(&mut self) {
        self.peer.pending.lock().unwrap().remove(&self.id);
        if !self.completed {
            self.peer.close();
        }
    }
}

fn encode(value: &Value) -> Result<Vec<u8>> {
    let mut encoded = serde_json::to_vec(value)?;
    if encoded.len() > MAX_FRAME_BYTES
        || (value["method"] == "submit" && encoded.len() > MAX_CONTROL_FRAME_BYTES)
    {
        return Err(Error::Domain("Pi RPC frame exceeds size limit".into()));
    }
    encoded.push(b'\n');
    Ok(encoded)
}

async fn read_frame<R: tokio::io::AsyncBufRead + Unpin>(stream: &mut R) -> Result<Vec<u8>> {
    let mut frame = Vec::new();
    loop {
        let available = stream.fill_buf().await?;
        if available.is_empty() {
            return Err(Error::CapabilityUnavailable("Pi connection closed".into()));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let size = newline.unwrap_or(available.len());
        if frame.len() + size > MAX_FRAME_BYTES {
            return Err(Error::Domain("Pi RPC frame exceeds size limit".into()));
        }
        frame.extend_from_slice(&available[..size]);
        stream.consume(size + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(frame);
        }
    }
}

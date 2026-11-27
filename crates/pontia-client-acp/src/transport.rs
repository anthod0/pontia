use base64::{Engine, engine::general_purpose::STANDARD};
use pontia_agent_control::{
    ClientControlChannel, ClientControlOperation,
    ipc::rpc::{MAX_FRAME_BYTES, Requests, RpcPeer},
};
use pontia_core::{Error, Result};
use pontia_runtime::{ProcessFingerprint, ProcessObservation};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf},
    sync::watch,
};

#[derive(Default)]
struct Written {
    keys: HashSet<String>,
    responses: HashSet<String>,
    steering: HashMap<String, String>,
    closed: bool,
    rejected: HashSet<String>,
}

pub(crate) struct Receipts {
    state: Mutex<Written>,
    changed: watch::Sender<u64>,
}
impl Receipts {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(Written::default()),
            changed: watch::channel(0).0,
        })
    }
    pub async fn wait(&self, key: String) -> Result<()> {
        let mut changes = self.changed.subscribe();
        loop {
            {
                let mut state = self.state.lock().unwrap();
                if state.keys.remove(&key) {
                    return Ok(());
                }
                if state.rejected.remove(&key) {
                    return Err(if key.starts_with("response:") {
                        Error::StateConflict("ACP permission was withdrawn".into())
                    } else {
                        crate::steering::prompt_required()
                    });
                }
                if state.closed {
                    return Err(Error::ControlUnknown(
                        "ACP delivery could not be confirmed".into(),
                    ));
                }
            }
            changes
                .changed()
                .await
                .map_err(|_| Error::ControlUnknown("ACP connection closed".into()))?;
        }
    }
    pub fn response(&self, id: &agent_client_protocol::v1::schema::RequestId) -> Result<String> {
        let key = format!("response:{}", serde_json::to_string(id)?);
        self.state.lock().unwrap().responses.insert(key.clone());
        Ok(key)
    }
    pub fn steering_request(&self, key: String, turn: &str) {
        self.state.lock().unwrap().steering.insert(key, turn.into());
    }

    fn rejected(&self, frame: &Value) {
        let id = &frame["id"];
        if frame["method"] == "session/prompt" || frame["method"] == "_session/steer" {
            let mut state = self.state.lock().unwrap();
            state.rejected.insert(id.to_string());
            self.changed.send_modify(|version| *version += 1);
            return;
        }
        let key = format!("response:{id}");
        let mut state = self.state.lock().unwrap();
        if state.responses.remove(&key) {
            state.rejected.insert(key);
            self.changed.send_modify(|version| *version += 1);
        }
    }
    pub fn close(&self) {
        self.state.lock().unwrap().closed = true;
        self.changed.send_modify(|version| *version += 1);
    }
    fn written(&self, frame: &Value) {
        let key = if let Some(id) = frame.get("id").filter(|_| frame.get("method").is_some()) {
            Some(id.to_string())
        } else if let Some(id) = frame.get("id") {
            Some(format!("response:{id}"))
        } else if frame["method"] == "session/cancel" {
            Some("session/cancel".into())
        } else {
            None
        };
        if let Some(key) = key {
            let mut state = self.state.lock().unwrap();
            if !key.starts_with("response:") || state.responses.remove(&key) {
                state.keys.insert(key);
                self.changed.send_modify(|version| *version += 1);
            }
        }
    }
}

pub(crate) struct Relay {
    pub rpc: Arc<RpcPeer>,
    pub process: ProcessFingerprint,
}
impl Relay {
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let response = self
            .rpc
            .request(method, params, MAX_FRAME_BYTES, false)
            .await?;
        response
            .get("result")
            .cloned()
            .ok_or_else(|| Error::ControlUnknown("Relay operation was not confirmed".into()))
    }
    pub async fn terminate(&self) -> Result<()> {
        if !self.rpc.is_closed() {
            // Native session/close may have already ended the Agent and its relay.
            let _ = self.call("relay.exit", json!({})).await;
        }
        if self.process.observe() != ProcessObservation::Exited {
            pontia_runtime::GenericRuntimeManager.terminate_process(&self.process)?;
        }
        tokio::time::timeout(Duration::from_secs(10), async {
            while self.process.observe() != ProcessObservation::Exited {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| Error::ControlUnknown("ACP relay exit unconfirmed".into()))
    }
}
impl ClientControlChannel for Relay {
    fn start_turn<'a>(
        &'a self,
        _: &'a pontia_agent_control::InputPayload,
        _: Option<&'a str>,
    ) -> pontia_agent_control::DeliveryOperation<'a> {
        Box::pin(async {
            Err(pontia_agent_control::DeliveryError::Unsupported(
                "ACP input requires a native Session".into(),
            ))
        })
    }
    fn process_id(&self) -> Option<u32> {
        Some(self.process.pid)
    }
    fn available(&self) -> bool {
        !self.rpc.is_closed()
    }
    fn invalidate(&self) {
        self.rpc.close();
    }
    fn ping(&self) -> ClientControlOperation<'_> {
        Box::pin(async { self.call("relay.status", json!({})).await.map(|_| ()) })
    }
    fn shutdown(&self) -> ClientControlOperation<'_> {
        Box::pin(self.terminate())
    }
    fn replay<'a>(&'a self, _: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async {
            Err(Error::CapabilityUnavailable(
                "ACP input replay is unavailable".into(),
            ))
        })
    }
}

pub(crate) const FRAME_KEY: &str = "pontia.dev/relay-frame";

pub(crate) fn frame_key(meta: Option<&agent_client_protocol::v1::schema::Meta>) -> Result<String> {
    meta.and_then(|meta| meta.get(FRAME_KEY))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::Domain("Missing ACP observation frame identity".into()))
}

// The SDK sees native ACP frames. Only the relay's confirmed pipe flush releases a receipt.
pub(crate) async fn bridge(
    relay: Arc<Relay>,
    mut requests: Requests,
    receipts: Arc<Receipts>,
    streams: (WriteHalf<DuplexStream>, ReadHalf<DuplexStream>),
    associated: Arc<std::sync::OnceLock<std::sync::Weak<crate::peer::Peer>>>,
    recovered_request: Option<Value>,
    ready: watch::Sender<bool>,
) -> Result<()> {
    let (mut writer, reader) = streams;
    let permission_keys = Mutex::new(std::collections::HashMap::<String, String>::new());
    let outgoing = async {
        let mut reader = BufReader::new(reader);
        loop {
            let mut bytes = Vec::new();
            if reader.read_until(b'\n', &mut bytes).await? == 0 {
                return Ok(());
            }
            let frame: Value = serde_json::from_slice(&bytes)?;
            let permission = if frame.get("method").is_none() {
                permission_keys
                    .lock()
                    .unwrap()
                    .remove(&frame["id"].to_string())
            } else {
                None
            };
            let steering_turn =
                if frame["method"] == "session/prompt" || frame["method"] == "_session/steer" {
                    receipts
                        .state
                        .lock()
                        .unwrap()
                        .steering
                        .remove(&frame["id"].to_string())
                } else {
                    None
                };
            let count = bytes.len().div_ceil(64 * 1024);
            let mut written = true;
            for (i, chunk) in bytes.chunks(64 * 1024).enumerate() {
                let mut params = json!({"chunk":STANDARD.encode(chunk)});
                if i == 0 && frame["method"] == "session/close" {
                    params["close_request_id"] = frame["id"].clone();
                }
                let method = if let Some(turn) = &steering_turn {
                    params["turn"] = json!(turn);
                    params["last"] = json!(i + 1 == count);
                    "relay.steer_prompt"
                } else {
                    "relay.write"
                };
                if let Some(key) = &permission {
                    params["permission"] = json!({"id":frame["id"],"key":key,"last":i+1==count});
                }
                if relay.call(method, params).await?["written"] == false {
                    written = false;
                    break;
                }
            }
            if written {
                receipts.written(&frame);
            } else {
                receipts.rejected(&frame);
            }
        }
    };
    let incoming = async {
        let mut partial = std::collections::HashMap::<String, Vec<u8>>::new();
        while let Some(request) = requests.recv().await {
            if request.method == "relay.ready" {
                ready.send_replace(true);
                relay.rpc.reply(request.id, json!({})).await?;
                continue;
            }
            if request.method == "relay.report" {
                let result = match associated.get().and_then(std::sync::Weak::upgrade) {
                    Some(peer) => match serde_json::from_value(request.params) {
                        Ok(report) => peer.apply_report(report).await,
                        Err(error) => Err(error.into()),
                    },
                    None => Err(Error::ControlUnknown("ACP peer unavailable".into())),
                };
                match result {
                    Ok(()) => {
                        relay
                            .rpc
                            .reply(request.id, json!({"accepted":true}))
                            .await?
                    }
                    Err(error) => {
                        let code = match error {
                            Error::StateConflict(_)
                            | Error::Domain(_)
                            | Error::NotFound(_)
                            | Error::Serialization(_) => -32009,
                            _ => -32603,
                        };
                        tracing::warn!(%error,"ACP report rejected");
                        relay
                            .rpc
                            .reply_error(request.id, code, &error.to_string())
                            .await?;
                    }
                }
                continue;
            }
            if request.method != "relay.frame" {
                return Err(Error::Domain("Unknown ACP relay method".into()));
            }
            let chunk = STANDARD
                .decode(
                    request.params["chunk"]
                        .as_str()
                        .ok_or_else(|| Error::Domain("Relay output requires bytes".into()))?,
                )
                .map_err(|_| Error::Domain("Invalid relay output bytes".into()))?;
            let id = request.params["frame_id"]
                .as_str()
                .ok_or_else(|| Error::Domain("Missing ACP frame identity".into()))?;
            partial.entry(id.into()).or_default().extend(chunk);
            if request.params["last"] == true {
                let mut bytes = partial.remove(id).expect("current frame");
                let mut frame: Value = serde_json::from_slice(&bytes)?;
                if frame["method"] == "session/request_permission" {
                    let peer = associated
                        .get()
                        .and_then(std::sync::Weak::upgrade)
                        .ok_or_else(|| {
                            Error::ControlUnknown(
                                "Permission arrived before relay attachment".into(),
                            )
                        })?;
                    let key = request.params["frame_id"]
                        .as_str()
                        .ok_or_else(|| Error::Domain("Missing permission frame identity".into()))?;
                    let key = format!("acp:frame:{key}");
                    match crate::interactions::permission_frame(&peer, &key, &mut frame).await {
                        Ok(Some((id, ended))) => {
                            relay
                                .rpc
                                .reply(request.id, json!({"interaction_id": id, "ended":ended}))
                                .await?;
                            bytes.clear();
                            continue;
                        }
                        Ok(None) => {
                            permission_keys
                                .lock()
                                .unwrap()
                                .insert(frame["id"].to_string(), key.clone());
                            let registering = peer
                                .registering_permissions
                                .lock()
                                .unwrap()
                                .get(&key)
                                .map(|done| done.subscribe());
                            bytes = serde_json::to_vec(&frame)?;
                            bytes.push(b'\n');
                            writer.write_all(&bytes).await?;
                            if let Some(mut done) = registering {
                                let _ = done.changed().await;
                                match peer.interactions.registration(&peer.session, &key).await {
                                    Ok(Some((stored, _))) => {
                                        relay
                                            .rpc
                                            .reply(
                                                request.id,
                                                json!({"interaction_id":stored.interaction_id}),
                                            )
                                            .await?
                                    }
                                    Ok(None) => {
                                        relay
                                            .rpc
                                            .reply_error(
                                                request.id,
                                                -32009,
                                                "ACP permission registration rejected",
                                            )
                                            .await?
                                    }
                                    Err(error) => {
                                        relay
                                            .rpc
                                            .reply_error(request.id, -32603, &error.to_string())
                                            .await?
                                    }
                                }
                            } else {
                                relay.rpc.reply(request.id, json!({})).await?;
                            }
                            continue;
                        }
                        Err(error) => {
                            tracing::warn!(session=%peer.session, %key, %error, "ACP permission frame rejected");
                            let code =
                                if pontia_agent_control::interactions::transient_storage_error(
                                    &error,
                                ) {
                                    -32603
                                } else {
                                    -32009
                                };
                            relay
                                .rpc
                                .reply_error(request.id, code, &error.to_string())
                                .await?;
                            bytes.clear();
                            continue;
                        }
                    }
                }
                if recovered_request
                    .as_ref()
                    .is_some_and(|id| frame.get("id") == Some(id))
                    && frame.get("method").is_none()
                {
                    bytes = serde_json::to_vec(
                        &json!({"jsonrpc":"2.0","method":"pontia/recovered-result","params":frame}),
                    )?;
                    bytes.push(b'\n');
                }
                writer.write_all(&bytes).await?;
                bytes.clear();
            }
            relay.rpc.reply(request.id, json!({})).await?;
        }
        Ok(())
    };
    let result = tokio::select! { result = outgoing => result, result = incoming => result };
    receipts.close();
    relay.rpc.close();
    result
}

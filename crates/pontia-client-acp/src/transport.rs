use base64::{Engine, engine::general_purpose::STANDARD};
use pontia_agent_control::{
    ClientControlChannel, ClientControlOperation,
    ipc::rpc::{MAX_FRAME_BYTES, Requests, RpcPeer},
};
use pontia_core::{Error, Result};
use pontia_runtime::{ProcessFingerprint, ProcessObservation};
use serde_json::{Value, json};
use std::{
    collections::HashSet,
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
    closed: bool,
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
    fn interrupt(&self) -> ClientControlOperation<'_> {
        Box::pin(async {
            Err(Error::CapabilityUnavailable(
                "ACP interrupt requires its native Session".into(),
            ))
        })
    }
    fn replay<'a>(&'a self, _: &'a str) -> ClientControlOperation<'a> {
        Box::pin(async {
            Err(Error::CapabilityUnavailable(
                "ACP input replay is unavailable".into(),
            ))
        })
    }
    fn submit<'a>(&'a self, _: &'a str, _: Option<&'a str>) -> ClientControlOperation<'a> {
        Box::pin(async {
            Err(Error::CapabilityUnavailable(
                "ACP input requires a Turn".into(),
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
    let outgoing = async {
        let mut reader = BufReader::new(reader);
        loop {
            let mut bytes = Vec::new();
            if reader.read_until(b'\n', &mut bytes).await? == 0 {
                return Ok(());
            }
            let frame: Value = serde_json::from_slice(&bytes)?;
            for chunk in bytes.chunks(64 * 1024) {
                relay
                    .call("relay.write", json!({"chunk":STANDARD.encode(chunk)}))
                    .await?;
            }
            receipts.written(&frame);
        }
    };
    let incoming = async {
        let mut bytes = Vec::new();
        while let Some(request) = requests.recv().await {
            if request.method == "relay.ready" {
                ready.send_replace(true);
                relay.rpc.reply(request.id, json!({})).await?;
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
            bytes.extend(chunk);
            if request.params["last"] == true {
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
                        Ok(Some(id)) => {
                            relay
                                .rpc
                                .reply(request.id, json!({"interaction_id": id}))
                                .await?;
                            bytes.clear();
                            continue;
                        }
                        Ok(None) => {
                            bytes = serde_json::to_vec(&frame)?;
                            bytes.push(b'\n');
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
                let tagged = if frame["method"] == "session/update"
                    && matches!(
                        frame
                            .pointer("/params/update/sessionUpdate")
                            .and_then(Value::as_str),
                        Some("usage_update" | "config_option_update")
                    ) {
                    frame.get_mut("params")
                } else if frame.pointer("/result/configOptions").is_some() {
                    frame.get_mut("result")
                } else {
                    None
                };
                if let Some(body) = tagged {
                    let id = request.params["frame_id"].as_str().ok_or_else(|| {
                        Error::Domain("Relay output requires a frame identity".into())
                    })?;
                    // Carry the trusted relay identity through the SDK's typed responses.
                    let meta = &mut body["_meta"];
                    if !meta.is_object() {
                        *meta = json!({});
                    }
                    meta[FRAME_KEY] = json!(format!("acp:frame:{id}"));
                    bytes = serde_json::to_vec(&frame)?;
                    bytes.push(b'\n');
                } else if recovered_request
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

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::v1::schema::SessionNotification;
    use tokio::net::UnixStream;

    #[tokio::test]
    async fn replayed_usage_frame_keeps_its_identity_and_native_contents() {
        let (client, server) = UnixStream::pair().unwrap();
        let (remote, _) = RpcPeer::new(client);
        let (rpc, requests) = RpcPeer::new(server);
        let relay = Arc::new(Relay {
            rpc,
            process: ProcessFingerprint::capture(std::process::id()).unwrap(),
        });
        let (sdk, stream) = tokio::io::duplex(4096);
        let (reader, writer) = tokio::io::split(stream);
        let task = tokio::spawn(bridge(
            relay,
            requests,
            Receipts::new(),
            (writer, reader),
            Arc::default(),
            None,
            watch::channel(false).0,
        ));
        let mut frames = BufReader::new(sdk).lines();
        let update = json!({"sessionUpdate":"usage_update","used":9,"size":10});
        let frame = json!({"jsonrpc":"2.0","method":"session/update","params":{
            "sessionId":"native","update":update,"_meta":{FRAME_KEY:"agent-value"}
        }});
        let input = json!({"frame_id":"generation:17","last":true,"chunk":STANDARD.encode(frame.to_string())});
        for _ in 0..2 {
            remote
                .request("relay.frame", input.clone(), MAX_FRAME_BYTES, false)
                .await
                .unwrap();
            let frame: Value =
                serde_json::from_str(&frames.next_line().await.unwrap().unwrap()).unwrap();
            assert_eq!(frame["params"]["update"], update);
            let notification: SessionNotification =
                serde_json::from_value(frame["params"].clone()).unwrap();
            assert_eq!(
                frame_key(notification.meta.as_ref()).unwrap(),
                "acp:frame:generation:17"
            );
        }
        remote.close();
        task.await.unwrap().unwrap();
    }
}

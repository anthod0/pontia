use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    sync::{Mutex, watch},
};

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PiControlEndpoint {
    pub runtime_instance_id: String,
    pub socket_path: String,
    pub version: u32,
}

impl PiControlEndpoint {
    pub fn validate(&self) -> Result<()> {
        if self.version != PROTOCOL_VERSION {
            return Err(Error::Domain(
                "unsupported Pi control protocol version".into(),
            ));
        }
        if self.runtime_instance_id.is_empty()
            || !Path::new(&self.socket_path).is_absolute()
            || self.socket_path.contains('\0')
            || self.socket_path.len() > 103
        {
            return Err(Error::Domain(
                "Pi control requires a runtime identity and an absolute socket path of at most 103 bytes without NUL".into(),
            ));
        }
        Ok(())
    }
}

/// One serialized request stream for one Pi running instance. Failed requests are never replayed.
pub struct PiControlConnection {
    session_id: String,
    endpoint: PiControlEndpoint,
    stream: Mutex<Option<BufReader<UnixStream>>>,
    invalidated: watch::Sender<bool>,
    sequence: AtomicU64,
}

impl PiControlConnection {
    pub fn new(session_id: String, endpoint: PiControlEndpoint) -> Result<Self> {
        endpoint.validate()?;
        Ok(Self {
            session_id,
            endpoint,
            stream: Mutex::new(None),
            invalidated: watch::channel(false).0,
            sequence: AtomicU64::new(0),
        })
    }

    pub fn endpoint(&self) -> &PiControlEndpoint {
        &self.endpoint
    }

    pub fn invalidate(&self) {
        self.invalidated.send_replace(true);
        if let Ok(mut stream) = self.stream.try_lock() {
            stream.take();
        }
    }

    pub async fn ping(&self) -> Result<()> {
        let result = self.request("ping", json!({})).await?;
        if result != json!({ "pong": true }) {
            return Err(Error::Domain("invalid Pi control ping response".into()));
        }
        Ok(())
    }

    pub async fn submit(&self, input: &str, inbox_message_id: Option<&str>) -> Result<()> {
        let result = self
            .request(
                "submit",
                json!({
                    "input": input, "inbox_message_id": inbox_message_id,
                }),
            )
            .await?;
        if result != json!({ "accepted": true }) {
            return Err(Error::Domain(
                "invalid Pi control submit response; delivery may be uncertain".into(),
            ));
        }
        Ok(())
    }

    async fn request(&self, method: &str, payload: Value) -> Result<Value> {
        let mut invalidated = self.invalidated.subscribe();
        if *invalidated.borrow() {
            return Err(Error::StateConflict(
                "Pi control binding has changed".into(),
            ));
        }
        tokio::select! {
            biased;
            _ = invalidated.changed() => Err(Error::StateConflict("Pi control binding has changed".into())),
            result = tokio::time::timeout(REQUEST_TIMEOUT, self.request_inner(method, payload)) => {
                result.map_err(|_| Error::Domain("Pi control request timed out; request was not replayed".into()))?
            }
        }
    }

    async fn request_inner(&self, method: &str, payload: Value) -> Result<Value> {
        let mut slot = self.stream.lock().await;
        // Keep the stream outside the slot during I/O so cancellation also disconnects it.
        let mut stream = match slot.take() {
            Some(stream) => stream,
            None => {
                let mut stream =
                    BufReader::new(UnixStream::connect(&self.endpoint.socket_path).await?);
                let hello = self
                    .exchange(
                        &mut stream,
                        "hello",
                        json!({
                            "session_id": self.session_id,
                            "runtime_instance_id": self.endpoint.runtime_instance_id,
                        }),
                    )
                    .await?;
                if hello
                    != json!({
                        "session_id": self.session_id,
                        "runtime_instance_id": self.endpoint.runtime_instance_id,
                    })
                {
                    return Err(Error::StateConflict(
                        "Pi control handshake identity mismatch".into(),
                    ));
                }
                stream
            }
        };
        let result = self.exchange(&mut stream, method, payload).await?;
        *slot = Some(stream);
        Ok(result)
    }

    async fn exchange(
        &self,
        stream: &mut BufReader<UnixStream>,
        method: &str,
        mut request: Value,
    ) -> Result<Value> {
        let request_id = self.sequence.fetch_add(1, Ordering::Relaxed).to_string();
        request["request_id"] = json!(request_id);
        request["version"] = json!(PROTOCOL_VERSION);
        request["method"] = json!(method);
        let mut encoded = serde_json::to_vec(&request)?;
        if encoded.len() > MAX_FRAME_BYTES {
            return Err(Error::Domain("Pi control request exceeds 64 KiB".into()));
        }
        encoded.push(b'\n');
        stream.get_mut().write_all(&encoded).await?;
        let response: Value = serde_json::from_slice(&read_frame(stream).await?)?;
        if response["version"] != PROTOCOL_VERSION {
            return Err(Error::Domain("invalid Pi control response version".into()));
        }
        if response["request_id"] != request_id {
            if response["request_id"].is_null() && response["error"]["code"] == "connection_busy" {
                return Err(Error::Domain(
                    "Pi control connection_busy: endpoint already has a controller".into(),
                ));
            }
            return Err(Error::Domain(
                "Pi control response request_id mismatch".into(),
            ));
        }
        if let Some(error) = response.get("error") {
            return Err(Error::Domain(format!(
                "Pi control {}: {}",
                error["code"], error["message"]
            )));
        }
        response
            .get("result")
            .cloned()
            .ok_or_else(|| Error::Domain("Pi control response missing result".into()))
    }
}

async fn read_frame(stream: &mut BufReader<UnixStream>) -> Result<Vec<u8>> {
    let mut frame = Vec::new();
    loop {
        let available = stream.fill_buf().await?;
        if available.is_empty() {
            return Err(Error::Domain(
                "Pi control connection closed before response".into(),
            ));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let size = newline.unwrap_or(available.len());
        if frame.len() + size > MAX_FRAME_BYTES {
            return Err(Error::Domain("Pi control response exceeds 64 KiB".into()));
        }
        frame.extend_from_slice(&available[..size]);
        stream.consume(size + usize::from(newline.is_some()));
        if newline.is_some() {
            return Ok(frame);
        }
    }
}

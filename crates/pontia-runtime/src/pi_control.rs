use std::{
    path::Path,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
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

pub const PROTOCOL_VERSION: u32 = 2;
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
        self.request("ping", json!({}), json!({ "pong": true }))
            .await
    }

    pub async fn submit(&self, input: &str, inbox_message_id: Option<&str>) -> Result<()> {
        self.request(
            "submit",
            json!({ "input": input, "inbox_message_id": inbox_message_id }),
            json!({ "accepted": true }),
        )
        .await
    }

    async fn request(&self, method: &str, payload: Value, expected: Value) -> Result<()> {
        let mut invalidated = self.invalidated.subscribe();
        if *invalidated.borrow() {
            return Err(Error::StateConflict(
                "Pi control binding has changed".into(),
            ));
        }
        let sent = AtomicBool::new(false);
        let uncertain = |message: &str| {
            if sent.load(Ordering::Acquire) {
                Error::ControlUnknown(message.into())
            } else {
                Error::CapabilityUnavailable(message.into())
            }
        };
        tokio::select! {
            biased;
            _ = invalidated.changed() => Err(uncertain("Pi control binding has changed")),
            result = tokio::time::timeout(REQUEST_TIMEOUT, self.request_inner(method, payload, expected, &sent)) => {
                result.map_err(|_| uncertain("Pi control request timed out; request was not replayed"))?
            }
        }
    }

    async fn request_inner(
        &self,
        method: &str,
        payload: Value,
        expected: Value,
        sent: &AtomicBool,
    ) -> Result<()> {
        let mut slot = self.stream.lock().await;
        // Keep the stream outside the slot during I/O so cancellation also disconnects it.
        let mut stream = match slot.take() {
            Some(stream) => stream,
            None => {
                let mut stream =
                    BufReader::new(UnixStream::connect(&self.endpoint.socket_path).await?);
                self.sequence.store(0, Ordering::Relaxed);
                let hello = self
                    .exchange(
                        &mut stream,
                        "hello",
                        json!({
                            "session_id": self.session_id,
                            "runtime_instance_id": self.endpoint.runtime_instance_id,
                        }),
                        None,
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
        let result = self
            .exchange(&mut stream, method, payload, Some(sent))
            .await?;
        if result != expected {
            return Err(Error::ControlUnknown(format!(
                "invalid Pi control {method} response; delivery may be uncertain"
            )));
        }
        *slot = Some(stream);
        Ok(())
    }

    async fn exchange(
        &self,
        stream: &mut BufReader<UnixStream>,
        method: &str,
        params: Value,
        sent: Option<&AtomicBool>,
    ) -> Result<Value> {
        let request_id = self.sequence.fetch_add(1, Ordering::Relaxed);
        let request =
            json!({ "jsonrpc": "2.0", "id": request_id, "method": method, "params": params });
        let mut encoded = serde_json::to_vec(&request)?;
        if encoded.len() > MAX_FRAME_BYTES {
            return Err(Error::Domain("Pi control request exceeds 64 KiB".into()));
        }
        encoded.push(b'\n');
        if let Some(sent) = sent {
            sent.store(true, Ordering::Release);
        }
        let exchange_error = |error: Error| {
            if sent.is_some() {
                Error::ControlUnknown(error.to_string())
            } else {
                error
            }
        };
        stream
            .get_mut()
            .write_all(&encoded)
            .await
            .map_err(Error::from)
            .map_err(exchange_error)?;
        let response: Value =
            serde_json::from_slice(&read_frame(stream).await.map_err(exchange_error)?)
                .map_err(Error::from)
                .map_err(exchange_error)?;
        if response["jsonrpc"] != "2.0"
            || response.get("id").is_none()
            || response.get("result").is_some() == response.get("error").is_some()
            || response.get("method").is_some()
        {
            return Err(exchange_error(Error::Domain(
                "invalid Pi control JSON-RPC response".into(),
            )));
        }
        if let Some(error) = response.get("error")
            && (!error["code"].is_i64() || !error["message"].is_string())
        {
            return Err(exchange_error(Error::Domain(
                "invalid Pi control JSON-RPC error".into(),
            )));
        }
        if response["id"] != request_id {
            if sent.is_none() && response["id"].is_null() && response["error"]["code"] == -32001 {
                return Err(Error::Domain(
                    "Pi control connection_busy: endpoint already has a controller".into(),
                ));
            }
            return Err(exchange_error(Error::Domain(
                "Pi control response id mismatch".into(),
            )));
        }
        if let Some(error) = response.get("error") {
            return Err(Error::Domain(format!(
                "Pi control {}: {}",
                error["code"], error["message"]
            )));
        }
        response.get("result").cloned().ok_or_else(|| {
            exchange_error(Error::Domain("Pi control response missing result".into()))
        })
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

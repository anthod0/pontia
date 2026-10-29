use pontia_application::EventIngestService;
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use std::{collections::HashMap, path::Path, process::Stdio, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixStream, unix::OwnedWriteHalf},
    process::Command,
    sync::{Mutex, oneshot, watch},
};

use crate::peer::Peer;
const MAX_FRAME: usize = 1024 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
type Reply = oneshot::Sender<Value>;

pub(crate) struct Connection {
    writer: Mutex<OwnedWriteHalf>,
    pending: Mutex<HashMap<u64, Reply>>,
    next: std::sync::atomic::AtomicU64,
    pub host_id: String,
    pub exited: watch::Receiver<bool>,
    stop: watch::Sender<bool>,
}

pub(crate) async fn frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Value> {
    let mut bytes = Vec::new();
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Err(Error::ControlUnknown(
                "Claude Agent control connection closed".into(),
            ));
        }
        let end = chunk.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(chunk.len(), |index| index + 1);
        if bytes.len() + count > MAX_FRAME {
            return Err(Error::Domain("Claude Agent frame too large".into()));
        }
        bytes.extend_from_slice(&chunk[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(serde_json::from_slice(&bytes)?);
        }
    }
}

impl Connection {
    pub async fn spawn(
        peer: Arc<Peer>,
        script: &Path,
        socket: &Path,
        environment: &std::collections::BTreeMap<String, String>,
        events: EventIngestService,
    ) -> Result<Arc<Self>> {
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let mut child = Command::new("node")
            .arg(script)
            .current_dir(&peer.cwd)
            .envs(environment)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let setup = async {
            let mut stdin = child.stdin.take().expect("piped stdin");
            stdin.write_all(format!("{}\n", json!({"session_id":peer.session,"runtime_id":peer.runtime,"cwd":peer.cwd,"socket_path":socket,"token":token})).as_bytes()).await?;
            drop(stdin);
            let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
            if frame(&mut stdout).await?["listening"] != true {
                return Err(Error::Domain("Claude Agent host did not start".into()));
            }
            let mut stream = UnixStream::connect(socket).await?;
            stream.write_all(format!("{}\n", json!({"method":"attach","session_id":peer.session,"runtime_id":peer.runtime,"token":token})).as_bytes()).await?;
            let (read, write) = stream.into_split();
            let mut reader = BufReader::new(read);
            let attached = frame(&mut reader).await?;
            if attached["type"] != "attached"
                || attached["sequence"] != 0
                || attached["high_water"] != 0
                || !attached["unavailable"].is_null()
            {
                return Err(Error::Domain(
                    "Claude Agent host attachment rejected".into(),
                ));
            }
            let host_id = attached["host_id"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| Error::Domain("Claude Agent host identity missing".into()))?
                .to_owned();
            Ok((reader, write, host_id))
        };
        let (mut reader, writer, host_id) = tokio::time::timeout(CONTROL_TIMEOUT, setup)
            .await
            .map_err(|_| {
            Error::CapabilityUnavailable("Claude Agent host startup timed out".into())
        })??;
        let (stop, mut stopping) = watch::channel(false);
        let (exit, exited) = watch::channel(false);
        let connection = Arc::new(Self {
            writer: Mutex::new(writer),
            pending: Mutex::new(HashMap::new()),
            next: std::sync::atomic::AtomicU64::new(1),
            host_id,
            exited,
            stop,
        });
        let connection_task = connection.clone();
        tokio::spawn(async move {
            let run = async {
                let mut sequence = 0;
                let read_result: Result<()> = async {
                    loop {
                        let value = frame(&mut reader).await?;
                        if value["host_id"] != connection_task.host_id {
                            return Err(Error::Domain("Claude Agent host identity changed".into()));
                        }
                        match value["type"].as_str() {
                            Some("response") => {
                                if let Some(id) = value["id"].as_u64()
                                    && let Some(reply) =
                                        connection_task.pending.lock().await.remove(&id)
                                {
                                    let _ = reply.send(value);
                                }
                            }
                            Some("event") => {
                                if value["sequence"].as_u64() != Some(sequence + 1) {
                                    return Err(Error::Domain("Claude Agent fact gap".into()));
                                }
                                peer.event(&events, &connection_task.host_id, &value["event"])
                                    .await?;
                                sequence += 1;
                            }
                            _ => return Err(Error::Domain("Claude Agent invalid frame".into())),
                        }
                    }
                }
                .await;
                if read_result.is_err() {
                    peer.unavailable("control_connection_lost").await;
                }
                connection_task.pending.lock().await.clear();
                let _ = connection_task.writer.lock().await.shutdown().await;
                let _ = child.wait().await;
                if let Err(error) = peer
                    .runtime_event(
                        &events,
                        pontia_application::PontiaEventType::RuntimeExited,
                        json!({"reason":"host_exited"}),
                    )
                    .await
                {
                    tracing::warn!(%error, "Claude Agent runtime exit report failed");
                }
            };
            let stopped = tokio::select! {
                _ = run => false,
                _ = stopping.wait_for(|stop| *stop) => true,
            };
            if stopped {
                // Ordinary owned-child shutdown, not a Session/Turn result.
                let _ = child.start_kill();
                let _ = child.wait().await;
            }
            // Only our host is owned here; SDK internals are never observed or signalled.
            peer.unavailable("host_unavailable").await;
            connection_task.pending.lock().await.clear();
            let _ = exit.send(true);
        });
        Ok(connection)
    }

    pub async fn request(&self, mut request: Value) -> Result<()> {
        // Allocate IDs under the writer lock: the host's high-water mark follows wire order.
        let mut writer = tokio::time::timeout(CONTROL_TIMEOUT, self.writer.lock())
            .await
            .map_err(|_| Error::ControlUnknown("Claude Agent writer unavailable".into()))?;
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        request["id"] = json!(id);
        request["host_id"] = json!(self.host_id);
        let bytes = format!("{request}\n");
        if bytes.len() > MAX_FRAME {
            return Err(Error::Domain(
                "Claude Agent input exceeds host frame limit".into(),
            ));
        }
        let (send, receive) = oneshot::channel();
        self.pending.lock().await.insert(id, send);
        let operation = async {
            writer
                .write_all(bytes.as_bytes())
                .await
                .map_err(|_| Error::ControlUnknown("Claude Agent write unconfirmed".into()))?;
            drop(writer);
            let response = receive
                .await
                .map_err(|_| Error::ControlUnknown("Claude Agent receipt lost".into()))?;
            match response["status"].as_str() {
                Some("accepted") => Ok(()),
                Some("rejected") if response["reason"] == "input_busy" => Err(Error::Conflict {
                    code: "input_busy",
                    message: "Claude Agent input is still active".into(),
                }),
                Some("rejected") => Err(Error::StateConflict(
                    "Claude Agent rejected control request".into(),
                )),
                Some("unsupported") => Err(Error::CapabilityUnavailable(
                    "Claude Agent operation unsupported".into(),
                )),
                _ => Err(Error::ControlUnknown(
                    "Claude Agent operation unconfirmed".into(),
                )),
            }
        };
        let result = tokio::time::timeout(CONTROL_TIMEOUT, operation)
            .await
            .unwrap_or_else(|_| {
                Err(Error::ControlUnknown(
                    "Claude Agent receipt timed out".into(),
                ))
            });
        self.pending.lock().await.remove(&id);
        result
    }

    pub async fn wait_exit(&self) -> Result<()> {
        let mut exited = self.exited.clone();
        tokio::time::timeout(CONTROL_TIMEOUT, exited.wait_for(|exited| *exited))
            .await
            .map_err(|_| Error::ControlUnknown("Claude Agent host exit unconfirmed".into()))?
            .map_err(|_| Error::ControlUnknown("Claude Agent host observation lost".into()))?;
        Ok(())
    }
    pub fn stop(&self) {
        self.stop.send_replace(true);
    }
}

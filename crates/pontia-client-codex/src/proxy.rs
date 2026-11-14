pub(crate) mod open;

use std::{
    collections::HashMap,
    io,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::{Path, PathBuf},
};

use futures_util::{SinkExt, StreamExt};
use pontia_core::Result;
use serde_json::Value;
use tokio::{
    net::{UnixListener, UnixStream},
    sync::{mpsc, watch},
    task::JoinSet,
};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::runtime::{daemon::DaemonIdentity, protocol};

pub(crate) struct Observation {
    pub daemon: DaemonIdentity,
    pub event: ObservedEvent,
}

pub(crate) enum ObservedEvent {
    ThreadOpened {
        runtime_id: String,
        operation: ThreadOperation,
        result: Box<Value>,
        params: Value,
        profile_binding: Option<Value>,
        reservation: Option<open::Reservation>,
    },
    ThreadUnsubscribed {
        thread: String,
        runtime_id: String,
    },
    ConnectionClosed(HashMap<String, String>),
    TurnStarted(String),
}

enum PendingRequest {
    Open(ThreadOperation, Value, Option<open::Prepared>),
    Unsubscribe(String),
}

pub(crate) fn unsubscribe_confirmed(result: &Value) -> bool {
    matches!(
        result["status"].as_str(),
        Some("unsubscribed" | "notSubscribed" | "notLoaded")
    )
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ThreadOperation {
    Start,
    Resume,
    Fork,
}

pub(crate) fn socket_path(home: &Path) -> PathBuf {
    home.join("codex-proxy.sock")
}

pub(crate) struct Proxy {
    listener: UnixListener,
    path: PathBuf,
}

impl Proxy {
    pub(crate) async fn bind(home: &Path) -> Result<Self> {
        let path = socket_path(home);
        if let Ok(metadata) = std::fs::symlink_metadata(&path) {
            if !metadata.file_type().is_socket() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "Codex proxy path is not a socket",
                )
                .into());
            }
            match UnixStream::connect(&path).await {
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    std::fs::remove_file(&path)?;
                }
                Err(error) => return Err(error.into()),
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "Codex proxy is already listening",
                    )
                    .into());
                }
            }
        }
        let listener = UnixListener::bind(&path)?;
        let proxy = Self { listener, path };
        std::fs::set_permissions(&proxy.path, std::fs::Permissions::from_mode(0o600))?;
        Ok(proxy)
    }

    pub(crate) async fn run(
        self,
        mut shutdown: watch::Receiver<bool>,
        upstream: impl Fn() -> Result<PathBuf>,
        observations: mpsc::Sender<Observation>,
        state: pontia_agent_control::AgentControl,
    ) -> Result<()> {
        let mut connections = JoinSet::new();
        while !*shutdown.borrow() {
            tokio::select! {
                _ = shutdown.changed() => break,
                result = connections.join_next(), if !connections.is_empty() => {
                    match result.expect("nonempty connection set") {
                        Ok(Ok(())) => {},
                        Ok(Err(error)) => tracing::debug!(%error, "Codex proxy connection closed"),
                        Err(error) => tracing::warn!(%error, "Codex proxy connection task failed"),
                    }
                }
                accepted = self.listener.accept() => {
                    let (downstream, _) = accepted?;
                    let endpoint = upstream();
                    let observations = observations.clone();
                    let state = state.clone();
                    connections.spawn(async move {
                        relay(downstream, &endpoint?, observations, state).await
                    });
                }
            }
        }
        connections.shutdown().await;
        Ok(())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_file(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "Could not remove Codex proxy socket");
        }
    }
}

async fn relay(
    downstream: UnixStream,
    upstream: &Path,
    observations: mpsc::Sender<Observation>,
    state: pontia_agent_control::AgentControl,
) -> Result<()> {
    let mut downstream = accept_async(downstream)
        .await
        .map_err(protocol::protocol_error)?;
    let mut upstream = protocol::open(upstream).await?;
    let daemon = DaemonIdentity::capture(upstream.get_ref())?;
    let mut pending = HashMap::<String, PendingRequest>::new();
    let mut runtimes = HashMap::<String, String>::new();
    enum PreparedFrame {
        Request(Message, String, Box<PendingRequest>),
        Response(Message),
    }
    let mut preparation = JoinSet::<Result<PreparedFrame>>::new();
    let cli_closed = loop {
        tokio::select! {
            completed = preparation.join_next(), if !preparation.is_empty() => {
                match completed.unwrap().map_err(protocol::protocol_error)?? {
                    PreparedFrame::Request(frame, id, request) => {
                        pending.insert(id, *request);
                        upstream.send(frame).await.map_err(protocol::protocol_error)?;
                    }
                    PreparedFrame::Response(frame) => {
                        downstream.send(frame).await.map_err(protocol::protocol_error)?;
                    }
                }
            }
            frame = downstream.next() => {
                let Some(frame) = frame else { break true };
                let frame = frame.map_err(protocol::protocol_error)?;
                if let Message::Text(text) = &frame
                    && let Ok(mut value) = serde_json::from_str::<Value>(text)
                    && let Some(id) = value.get("id").filter(|id| id.is_string() || id.is_number()).cloned()
                {
                    let operation = match value["method"].as_str() {
                        Some("thread/start") => Some(ThreadOperation::Start),
                        Some("thread/resume") => Some(ThreadOperation::Resume),
                        Some("thread/fork") => Some(ThreadOperation::Fork),
                        _ => None,
                    };
                    if let Some(operation) = operation {
                        let state = state.clone();
                        preparation.spawn(async move {
                            let prepared = open::prepare(&state, operation, &mut value["params"]).await?;
                            let pending = PendingRequest::Open(operation, value["params"].clone(), prepared);
                            Ok(PreparedFrame::Request(Message::text(value.to_string()), id.to_string(), Box::new(pending)))
                        });
                        continue;
                    } else if value["method"] == "thread/unsubscribe"
                        && let Some(thread) = value["params"]["threadId"].as_str() {
                        pending.insert(id.to_string(), PendingRequest::Unsubscribe(thread.into()));
                    }
                }
                let closed = frame.is_close();
                upstream.send(frame).await.map_err(protocol::protocol_error)?;
                if closed { break true; }
            }
            frame = upstream.next() => {
                let Some(frame) = frame else { break false };
                let frame = frame.map_err(protocol::protocol_error)?;
                if let Message::Text(text) = &frame
                    && let Ok(value) = serde_json::from_str::<Value>(text)
                {
                    let event = if value.get("method").is_none()
                        && let Some(id) = value.get("id")
                        && let Some(request) = pending.remove(&id.to_string())
                    {
                        match (request, value.get("result").filter(|_| value.get("error").is_none())) {
                            (PendingRequest::Open(operation, params, prepared), Some(result)) => {
                                if let Some(thread) = result["thread"]["id"].as_str() {
                                    let runtime_id = runtimes.entry(thread.into()).or_insert_with(|| pontia_core::ids::new_runtime_id().to_string()).clone();
                                    let (bound, ready) = tokio::sync::oneshot::channel();
                                    let reservation = prepared.as_ref().map(|prepared| open::Reservation { session: prepared.session.clone(), bound });
                                    let event = ObservedEvent::ThreadOpened { runtime_id, operation, result: Box::new(result.clone()), params,
                                        profile_binding: prepared.as_ref().and_then(|prepared| prepared.profile_binding.clone()), reservation };
                                    observations.send(Observation { daemon: daemon.clone(), event }).await
                                        .map_err(|_| protocol::protocol_error("proxy observation stopped"))?;
                                    preparation.spawn(async move {
                                        if let Some(prepared) = prepared
                                            && !prepared.bound {
                                            ready.await.map_err(|_| protocol::protocol_error("Codex proxy Session binding failed"))?;
                                        }
                                        Ok(PreparedFrame::Response(frame))
                                    });
                                    continue;
                                }
                                None
                            }
                            (PendingRequest::Unsubscribe(thread), Some(result)) if unsubscribe_confirmed(result) => {
                                runtimes.get(&thread).map(|runtime_id| ObservedEvent::ThreadUnsubscribed { thread, runtime_id: runtime_id.clone() })
                            }
                            _ => None,
                        }
                    } else if value["method"] == "turn/started" {
                        value["params"]["threadId"].as_str().map(|thread| ObservedEvent::TurnStarted(thread.into()))
                    } else { None };
                    if let Some(event) = event {
                        observations.send(Observation { daemon: daemon.clone(), event }).await
                            .map_err(|_| protocol::protocol_error("proxy observation stopped"))?;
                    }
                }
                let closed = frame.is_close();
                downstream.send(frame).await.map_err(protocol::protocol_error)?;
                if closed { break false; }
            }
        }
    };
    if cli_closed {
        observations
            .send(Observation {
                daemon,
                event: ObservedEvent::ConnectionClosed(runtimes),
            })
            .await
            .map_err(|_| protocol::protocol_error("proxy observation stopped"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;

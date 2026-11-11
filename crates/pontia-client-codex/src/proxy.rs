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
        operation: ThreadOperation,
        result: Value,
        params: Value,
        profile_binding: Option<Value>,
    },
    TurnStarted(String),
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
    let mut pending = HashMap::<String, (ThreadOperation, Value, Option<Value>)>::new();
    loop {
        tokio::select! {
            frame = downstream.next() => {
                let Some(frame) = frame else { break };
                let mut frame = frame.map_err(protocol::protocol_error)?;
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
                        let profile_binding = crate::service::profile::prepare_proxy_request(&state, operation, &mut value["params"]).await?;
                        pending.insert(id.to_string(), (operation, value["params"].clone(), profile_binding));
                        frame = Message::text(value.to_string());
                    }
                }
                let closed = frame.is_close();
                upstream.send(frame).await.map_err(protocol::protocol_error)?;
                if closed { break; }
            }
            frame = upstream.next() => {
                let Some(frame) = frame else { break };
                let frame = frame.map_err(protocol::protocol_error)?;
                if let Message::Text(text) = &frame
                    && let Ok(value) = serde_json::from_str::<Value>(text)
                {
                    let event = if value.get("method").is_none()
                        && let Some(id) = value.get("id")
                        && let Some((operation, params, profile_binding)) = pending.remove(&id.to_string())
                    {
                        value.get("result").filter(|_| value.get("error").is_none())
                            .cloned().map(|result| ObservedEvent::ThreadOpened { operation, result, params, profile_binding })
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
                if closed { break; }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;

mod daemon;
mod gateway;
pub mod protocol;

use pontia_core::{Error, Result};
use protocol::Connection;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
use tokio::sync::{Mutex, broadcast};

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<CodexRuntime>>> {
    static RUNTIMES: OnceLock<Mutex<HashMap<PathBuf, Arc<CodexRuntime>>>> = OnceLock::new();
    RUNTIMES.get_or_init(Default::default)
}

pub(crate) struct CurrentRuntimeGuard {
    _registry: tokio::sync::MutexGuard<'static, HashMap<PathBuf, Arc<CodexRuntime>>>,
}

pub struct CodexRuntime {
    pub root: PathBuf,
    pub instance_id: String,
    pub connection_id: String,
    pub socket_path: PathBuf,
    connection: Arc<Connection>,
    pub targets: broadcast::Sender<TuiTarget>,
    gateways: Mutex<HashMap<String, gateway::Gateway>>,
    operations: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    pub tui_targets: Mutex<HashMap<String, TuiTarget>>,
}

#[derive(Clone, Debug)]
pub struct TuiTarget {
    pub connection_id: String,
    pub owner_session_id: String,
    pub thread: Value,
    pub connected: bool,
}

impl CodexRuntime {
    pub async fn ensure(root: &Path) -> Result<Arc<Self>> {
        let root = root.canonicalize()?;
        let mut registry = registry().lock().await;
        if let Some(runtime) = registry.get(&root)
            && runtime.connection.is_connected()
        {
            return Ok(runtime.clone());
        }
        let endpoint = match registry.get(&root) {
            Some(runtime) => daemon::Endpoint {
                socket: runtime.socket_path.clone(),
                home: runtime.connection.codex_home.clone(),
            },
            None => daemon::Endpoint::resolve()?,
        };
        Self::connect(&mut registry, root, endpoint).await
    }

    async fn connect(
        registry: &mut HashMap<PathBuf, Arc<Self>>,
        root: PathBuf,
        endpoint: daemon::Endpoint,
    ) -> Result<Arc<Self>> {
        let connection = Connection::connect(&endpoint.socket).await?;
        if connection.codex_home.canonicalize().ok().as_ref() != Some(&endpoint.home) {
            connection.close().await;
            return Err(protocol::protocol_error(
                "daemon Codex home does not match the selected environment",
            ));
        }
        let instance_id = connection.identity.instance_id.clone();
        let socket_path = endpoint.socket;
        let (targets, _) = broadcast::channel(128);
        let runtime = Arc::new(Self {
            root: root.clone(),
            instance_id,
            connection_id: pontia_core::ids::new_runtime_instance_id().to_string(),
            socket_path,
            connection,
            targets,
            gateways: Mutex::new(HashMap::new()),
            operations: Mutex::new(HashMap::new()),
            tui_targets: Mutex::new(HashMap::new()),
        });
        if let Some(old) = registry.insert(root, runtime.clone()) {
            old.close_gateways().await;
            old.connection.close().await;
        }
        Ok(runtime)
    }

    // Hold replacement/shutdown off until the binding transaction commits.
    pub(crate) async fn current_guard(&self) -> Result<CurrentRuntimeGuard> {
        let guard = self.registered_guard().await?;
        if !self.connection.is_connected() {
            return Err(protocol::protocol_error(
                "daemon connection is no longer live",
            ));
        }
        Ok(guard)
    }

    pub(crate) async fn registered_guard(&self) -> Result<CurrentRuntimeGuard> {
        let registry = registry().lock().await;
        if !registry
            .get(&self.root)
            .is_some_and(|runtime| std::ptr::eq(runtime.as_ref(), self))
        {
            return Err(Error::StateConflict(
                "Codex runtime has been replaced".into(),
            ));
        }
        Ok(CurrentRuntimeGuard {
            _registry: registry,
        })
    }

    pub async fn connection(&self) -> Result<Arc<Connection>> {
        if !self.connection.is_connected() {
            return Err(protocol::protocol_error(
                "daemon disconnected; awaiting instance verification and reconciliation",
            ));
        }
        Ok(self.connection.clone())
    }

    pub async fn lock_session(&self, session: &str) -> tokio::sync::OwnedMutexGuard<()> {
        self.operations
            .lock()
            .await
            .entry(session.to_string())
            .or_default()
            .clone()
            .lock_owned()
            .await
    }

    pub async fn ensure_tui_gateway(self: &Arc<Self>, owner: &str) -> Result<()> {
        self.gateway(owner).await.map(|_| ())
    }

    async fn record_target(&self, target: TuiTarget) {
        let mut targets = self.tui_targets.lock().await;
        if !target.connected
            && targets
                .get(&target.owner_session_id)
                .is_some_and(|current| current.connection_id != target.connection_id)
        {
            return;
        }
        targets.insert(target.owner_session_id.clone(), target.clone());
        let _ = self.targets.send(target);
    }

    pub async fn shutdown(root: &Path) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if let Some(runtime) = registry().lock().await.remove(&root) {
            runtime.close_gateways().await;
            runtime.connection.close().await;
        }
    }

    async fn close_gateways(&self) {
        for (_, gateway) in self.gateways.lock().await.drain() {
            gateway.close().await;
        }
    }

    pub async fn open_tui(
        self: &Arc<Self>,
        owner: &str,
        thread_id: &str,
        cwd: &Path,
    ) -> Result<(String, String)> {
        let endpoint = self.gateway(owner).await?;
        let name = format!("pontia_codex_{}", owner.replace('-', "_"));
        if !pontia_runtime::is_alive(&name) {
            let binary = std::env::var("PONTIA_CODEX_COMMAND").unwrap_or_else(|_| "codex".into());
            let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
            let command = format!(
                "{} resume --remote {} {}",
                quote(&binary),
                quote(&endpoint),
                quote(thread_id)
            );
            if !pontia_runtime::spawn_tmux_session(&name, cwd, &command)?.success() {
                return Err(Error::Domain("could not open Codex TUI".into()));
            }
        }
        let pane = pontia_runtime::pane_binding(&name)
            .ok_or_else(|| Error::Domain("Codex TUI pane is missing".into()))?;
        Ok((pane.socket_path, pane.pane_id))
    }
}

#[cfg(test)]
mod tests;

mod daemon;
pub mod protocol;

use pontia_core::{Error, Result};
use protocol::Connection;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};
use tokio::sync::Mutex;

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<CodexRuntime>>> {
    static RUNTIMES: OnceLock<Mutex<HashMap<PathBuf, Arc<CodexRuntime>>>> = OnceLock::new();
    RUNTIMES.get_or_init(Default::default)
}

pub(crate) struct CurrentConnectionGuard {
    _registry: tokio::sync::MutexGuard<'static, HashMap<PathBuf, Arc<CodexRuntime>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubscriptionState {
    Reconciling,
    AwaitingFirstInput,
    Available,
    ExitPending,
}

pub struct CodexRuntime {
    pub root: PathBuf,
    pub socket_path: PathBuf,
    connection: Arc<Connection>,
    operations: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    subscriptions: Mutex<HashMap<String, SubscriptionState>>,
    pub(crate) profile_service: OnceLock<crate::profiles::CodexProfiles>,
}

pub async fn probe_daemon(codex_home: &Path) -> Result<()> {
    let home = codex_home
        .canonicalize()
        .map_err(protocol::protocol_error)?;
    let connection =
        Connection::connect(&home.join("app-server-control/app-server-control.sock")).await?;
    let reported_home = connection
        .codex_home
        .canonicalize()
        .map_err(protocol::protocol_error)?;
    if reported_home != home {
        connection.close().await;
        return Err(protocol::protocol_error(
            "daemon Codex home does not match the selected environment",
        ));
    }
    connection.close().await;
    Ok(())
}

impl CodexRuntime {
    pub async fn existing(root: &Path) -> Option<Arc<Self>> {
        let root = root.canonicalize().ok()?;
        registry().lock().await.get(&root).cloned()
    }

    #[cfg(test)]
    pub(crate) async fn install_for_test(root: &Path, socket: PathBuf) -> Result<Arc<Self>> {
        let root = root.canonicalize()?;
        let mut runtimes = registry().lock().await;
        Self::connect(
            &mut runtimes,
            root.clone(),
            daemon::Endpoint { socket, home: root },
        )
        .await
    }

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
        let runtime = Arc::new(Self {
            root: root.clone(),
            socket_path: endpoint.socket,
            connection,
            operations: Mutex::new(HashMap::new()),
            subscriptions: Mutex::new(HashMap::new()),
            profile_service: OnceLock::new(),
        });
        if let Some(old) = registry.insert(root, runtime.clone()) {
            old.connection.close().await;
        }
        Ok(runtime)
    }

    pub(crate) async fn current_guard(&self) -> Result<CurrentConnectionGuard> {
        let registry = registry().lock().await;
        if !registry
            .get(&self.root)
            .is_some_and(|runtime| std::ptr::eq(runtime.as_ref(), self))
            || !self.connection.is_connected()
        {
            return Err(Error::ControlUnknown(
                "Codex app-server connection was replaced".into(),
            ));
        }
        Ok(CurrentConnectionGuard {
            _registry: registry,
        })
    }

    pub fn codex_home(&self) -> &Path {
        &self.connection.codex_home
    }

    pub async fn connection(&self) -> Result<Arc<Connection>> {
        if !self.connection.is_connected() {
            return Err(protocol::protocol_error(
                "daemon disconnected; awaiting reconciliation",
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

    pub(crate) async fn set_subscription(&self, session: &str, state: SubscriptionState) {
        self.subscriptions
            .lock()
            .await
            .insert(session.into(), state);
    }

    pub(crate) async fn clear_subscription(&self, session: &str) {
        self.subscriptions.lock().await.remove(session);
    }

    pub(crate) async fn clear_subscriptions(&self) {
        self.subscriptions.lock().await.clear();
    }

    pub(crate) async fn subscription(&self, session: &str) -> Option<SubscriptionState> {
        self.subscriptions.lock().await.get(session).copied()
    }

    pub async fn shutdown(root: &Path) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        if let Some(runtime) = registry().lock().await.remove(&root) {
            runtime.connection.close().await;
        }
    }
}

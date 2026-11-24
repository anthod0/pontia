pub(crate) mod daemon;
pub mod protocol;

use pontia_core::{Error, Result};
use protocol::Connection;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodexServerTarget {
    User,
    Background,
}

impl CodexServerTarget {
    pub(crate) async fn for_session(pool: &sqlx::SqlitePool, session: &str) -> Result<Self> {
        use pontia_core::LaunchMode;
        Ok(
            match pontia_agent_control::ControlQueryService::new(pool.clone())
                .session_launch_mode(session)
                .await?
            {
                LaunchMode::Foreground => Self::User,
                LaunchMode::Background => Self::Background,
            },
        )
    }

    pub(super) fn launch_mode(self) -> pontia_core::LaunchMode {
        match self {
            Self::Background => pontia_core::LaunchMode::Background,
            Self::User => pontia_core::LaunchMode::Foreground,
        }
    }

    fn connection_gate(self) -> &'static Mutex<()> {
        static USER: Mutex<()> = Mutex::const_new(());
        static BACKGROUND: Mutex<()> = Mutex::const_new(());
        match self {
            Self::User => &USER,
            Self::Background => &BACKGROUND,
        }
    }
}

type Registry = HashMap<(PathBuf, CodexServerTarget), Arc<CodexRuntime>>;

fn registry() -> &'static Mutex<Registry> {
    static RUNTIMES: OnceLock<Mutex<Registry>> = OnceLock::new();
    RUNTIMES.get_or_init(Default::default)
}

pub(crate) struct CurrentConnectionGuard {
    _registry: tokio::sync::MutexGuard<'static, Registry>,
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
    pub target: CodexServerTarget,
    connection: Arc<Connection>,
    operations: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    pub(crate) proxy_peers: Arc<crate::proxy::peers::Peers>,
    subscriptions: Mutex<HashMap<String, (SubscriptionState, u64)>>,
    subscription_sequence: AtomicU64,
    pub(crate) instructions_service: OnceLock<crate::instructions::CodexInstructions>,
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

async fn connect_daemon(
    endpoint: &daemon::Endpoint,
    target: CodexServerTarget,
    unloaded: Arc<Mutex<HashMap<String, u64>>>,
) -> Result<Arc<Connection>> {
    let (socket, starting) = match target {
        CodexServerTarget::User => (protocol::open(&endpoint.socket).await?, None),
        CodexServerTarget::Background => {
            let (stream, starting) = daemon::connect_background(endpoint).await?;
            (protocol::websocket(stream).await?, starting)
        }
    };
    let connection = Connection::initialize(socket, unloaded, target).await?;
    if connection.codex_home.canonicalize().ok().as_ref() != Some(&endpoint.home) {
        connection.close().await;
        return Err(protocol::protocol_error(
            "daemon Codex home does not match the selected environment",
        ));
    }
    if let Some(starting) = starting {
        starting.ready(&endpoint.socket, &connection.identity)?;
    }
    Ok(connection)
}

impl CodexRuntime {
    pub(crate) async fn for_session(
        pool: &sqlx::SqlitePool,
        session: &str,
    ) -> Result<Option<Arc<Self>>> {
        let root: Option<String> = sqlx::query_scalar(
            "SELECT json_extract(metadata,'$.codex_control_root') FROM sessions WHERE session_id=?",
        )
        .bind(session)
        .fetch_optional(pool)
        .await?
        .flatten();
        let Some(root) = root else { return Ok(None) };
        Ok(Self::existing_target(
            Path::new(&root),
            CodexServerTarget::for_session(pool, session).await?,
        )
        .await)
    }

    pub(crate) async fn existing(root: &Path) -> Option<Arc<Self>> {
        Self::existing_target(root, CodexServerTarget::User).await
    }

    async fn existing_target(root: &Path, target: CodexServerTarget) -> Option<Arc<Self>> {
        registry()
            .lock()
            .await
            .get(&(root.to_owned(), target))
            .cloned()
    }

    #[cfg(test)]
    pub(crate) async fn install_for_test(root: &Path, socket: PathBuf) -> Result<Arc<Self>> {
        let root = root.canonicalize()?;
        let target = CodexServerTarget::User;
        let _guard = target.connection_gate().lock().await;
        Self::connect(
            root.clone(),
            target,
            daemon::Endpoint { socket, home: root },
        )
        .await
    }

    pub async fn ensure(root: &Path, target: CodexServerTarget) -> Result<Arc<Self>> {
        let root = root.canonicalize()?;
        let _guard = target.connection_gate().lock().await;
        let endpoint = if let Some(runtime) = Self::existing_target(&root, target).await {
            if runtime.connection.is_connected() {
                return Ok(runtime);
            }
            daemon::Endpoint {
                socket: runtime.socket_path.clone(),
                home: runtime.connection.codex_home.clone(),
            }
        } else {
            let mut endpoint = daemon::Endpoint::resolve()?;
            if target == CodexServerTarget::Background {
                endpoint.socket = root.join("state/codex/background-app-server.sock");
            }
            endpoint
        };
        Self::connect(root, target, endpoint).await
    }

    async fn connect(
        root: PathBuf,
        target: CodexServerTarget,
        endpoint: daemon::Endpoint,
    ) -> Result<Arc<Self>> {
        let previous = Self::existing_target(&root, target).await;
        let unloaded = previous
            .as_ref()
            .map(|runtime| runtime.connection.unloaded_threads.clone())
            .unwrap_or_default();
        let operations = previous
            .as_ref()
            .map(|runtime| runtime.operations.clone())
            .unwrap_or_default();
        let _launch = (target == CodexServerTarget::Background)
            .then(|| pontia_runtime::launch_lock::launch(&root))
            .transpose()?;
        let connection = connect_daemon(&endpoint, target, unloaded).await?;
        let runtime = Arc::new(Self {
            root: root.clone(),
            target,
            socket_path: endpoint.socket,
            operations,
            proxy_peers: previous
                .as_ref()
                .filter(|old| old.connection.identity == connection.identity)
                .map(|old| old.proxy_peers.clone())
                .unwrap_or_default(),
            connection,
            subscriptions: Mutex::new(HashMap::new()),
            subscription_sequence: AtomicU64::new(1),
            instructions_service: OnceLock::new(),
        });
        if let Some(old) = registry()
            .lock()
            .await
            .insert((root, target), runtime.clone())
        {
            old.connection.close().await;
        }
        Ok(runtime)
    }

    pub(crate) async fn current_guard(&self) -> Result<CurrentConnectionGuard> {
        let registry = registry().lock().await;
        if !registry
            .get(&(self.root.clone(), self.target))
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
        let operation = self
            .operations
            .lock()
            .await
            .entry(session.to_string())
            .or_default()
            .clone();
        operation.lock_owned().await
    }

    pub(crate) async fn set_subscription(&self, session: &str, state: SubscriptionState) {
        self.subscriptions.lock().await.insert(
            session.into(),
            (
                state,
                self.subscription_sequence.fetch_add(1, Ordering::Relaxed),
            ),
        );
    }

    pub(crate) async fn clear_subscription(&self, session: &str) {
        self.subscriptions.lock().await.remove(session);
    }

    pub(crate) async fn clear_subscriptions(&self) {
        self.subscriptions.lock().await.clear();
    }

    pub(crate) async fn subscription(&self, session: &str) -> Option<SubscriptionState> {
        self.subscriptions
            .lock()
            .await
            .get(session)
            .map(|(state, _)| *state)
    }

    pub(crate) async fn subscription_revision(&self, session: &str) -> Option<u64> {
        self.subscriptions
            .lock()
            .await
            .get(session)
            .map(|(_, revision)| *revision)
    }

    pub async fn shutdown(root: &Path) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        for target in [CodexServerTarget::User, CodexServerTarget::Background] {
            let _guard = target.connection_gate().lock().await;
            if let Some(runtime) = registry().lock().await.remove(&(root.clone(), target)) {
                runtime.connection.close().await;
            }
        }
    }
}

/// The owner can outlive its socket. A different listener may have since bound
/// the same path and must be reported independently.
pub struct BackgroundServer {
    pub owned: Vec<pontia_runtime::ProcessFingerprint>,
    pub unverified_listener: Option<(PathBuf, Option<u32>)>,
}

pub async fn background_server(root: &Path) -> Result<BackgroundServer> {
    use pontia_runtime::ProcessObservation;
    use std::io::ErrorKind;
    let socket = root.join("state/codex/background-app-server.sock");
    let owned = daemon::live_owners(&socket)?;
    let listener = match daemon::connect_socket(&socket).await {
        Ok(stream) => {
            let pid = stream
                .peer_cred()
                .ok()
                .and_then(|peer| peer.pid())
                .and_then(|pid| u32::try_from(pid).ok());
            if owned
                .iter()
                .any(|p| Some(p.pid) == pid && p.observe() == ProcessObservation::Alive)
            {
                None
            } else {
                Some((socket, pid))
            }
        }
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => None,
        Err(e) => return Err(e.into()),
    };
    Ok(BackgroundServer {
        owned,
        unverified_listener: listener,
    })
}

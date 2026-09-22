mod gateway;
pub mod protocol;

use pontia_core::{Error, Result, ids::new_runtime_instance_id};
use protocol::Connection;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    process::{Child, Command},
    sync::{Mutex, broadcast},
};

fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<CodexRuntime>>> {
    static RUNTIMES: OnceLock<Mutex<HashMap<PathBuf, Arc<CodexRuntime>>>> = OnceLock::new();
    RUNTIMES.get_or_init(Default::default)
}

pub struct CodexRuntime {
    pub root: PathBuf,
    pub instance_id: String,
    pub socket_path: PathBuf,
    child: Mutex<Child>,
    connection: Mutex<Arc<Connection>>,
    pub targets: broadcast::Sender<TuiTarget>,
    gateways: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
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
        std::fs::create_dir_all(root.join("state/codex"))?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            root.join("state/codex"),
            std::fs::Permissions::from_mode(0o700),
        )?;
        let root = root.canonicalize()?;
        let mut registry = registry().lock().await;
        if let Some(runtime) = registry.get(&root)
            && runtime.child.lock().await.try_wait()?.is_none()
        {
            return Ok(runtime.clone());
        }
        let binary = std::env::var("PONTIA_CODEX_COMMAND").unwrap_or_else(|_| "codex".into());
        let version = Command::new(&binary).arg("--version").output().await?;
        if !version.status.success()
            || String::from_utf8_lossy(&version.stdout).trim()
                != format!(
                    "codex-cli {}",
                    pontia_agent_clients::codex::SUPPORTED_VERSION
                )
        {
            return Err(Error::CapabilityUnavailable(format!(
                "Codex {} is required; set PONTIA_CODEX_COMMAND to its executable",
                pontia_agent_clients::codex::SUPPORTED_VERSION
            )));
        }
        let instance_id = new_runtime_instance_id().to_string();
        let socket_path = root
            .join("state/codex")
            .join(format!("{}.sock", &instance_id[instance_id.len() - 12..]));
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("state/codex/app-server.log"))?;
        let mut command = Command::new(&binary);
        command
            .args([
                "app-server",
                "--listen",
                &format!("unix://{}", socket_path.display()),
            ])
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .kill_on_drop(true);
        // A daemon crash must not leave a second server executing the same
        // persistent threads when the replacement daemon resumes them.
        #[cfg(target_os = "linux")]
        unsafe {
            let parent = std::process::id() as libc::pid_t;
            command.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    return Err(std::io::Error::other("Pontia exited before Codex started"));
                }
                Ok(())
            });
        }
        let mut child = command.spawn()?;
        let connection = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if child.try_wait()?.is_some() {
                    return Err(protocol::protocol_error("app-server exited during startup"));
                }
                if socket_path.exists() {
                    return Connection::connect(&socket_path).await;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| protocol::protocol_error("app-server startup timed out"))??;
        let (targets, _) = broadcast::channel(128);
        let runtime = Arc::new(Self {
            root: root.clone(),
            instance_id,
            socket_path,
            child: Mutex::new(child),
            connection: Mutex::new(connection),
            targets,
            gateways: Mutex::new(HashMap::new()),
            operations: Mutex::new(HashMap::new()),
            tui_targets: Mutex::new(HashMap::new()),
        });
        if let Some(old) = registry.insert(root, runtime.clone()) {
            old.close_gateways().await;
            old.connection.lock().await.close().await;
        }
        Ok(runtime)
    }

    pub async fn connection(&self) -> Result<Arc<Connection>> {
        let mut connection = self.connection.lock().await;
        if !connection.is_connected() {
            *connection = Connection::connect(&self.socket_path).await?;
        }
        Ok(connection.clone())
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
            runtime.connection.lock().await.close().await;
            let _ = runtime.child.lock().await.kill().await;
            let _ = std::fs::remove_file(&runtime.socket_path);
        }
    }

    async fn close_gateways(&self) {
        for (_, task) in self.gateways.lock().await.drain() {
            task.abort();
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
        if !crate::tmux::is_alive(&name) {
            let binary = std::env::var("PONTIA_CODEX_COMMAND").unwrap_or_else(|_| "codex".into());
            let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
            let command = format!(
                "{} resume --remote {} {}",
                quote(&binary),
                quote(&endpoint),
                quote(thread_id)
            );
            if !crate::tmux::spawn_tmux_session(&name, cwd, &command)?.success() {
                return Err(Error::Domain("could not open Codex TUI".into()));
            }
        }
        let pane = crate::tmux::pane_binding(&name)
            .ok_or_else(|| Error::Domain("Codex TUI pane is missing".into()))?;
        Ok((pane.socket_path, pane.pane_id))
    }
}

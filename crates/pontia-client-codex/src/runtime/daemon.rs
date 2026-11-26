use super::protocol::protocol_error;
use pontia_core::Result;
use std::path::PathBuf;
use tokio::net::UnixStream;

pub(crate) struct Endpoint {
    pub socket: PathBuf,
    pub home: PathBuf,
}

impl Endpoint {
    pub fn resolve() -> Result<Self> {
        let home = match std::env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
            Some(home) => PathBuf::from(home),
            None => std::env::home_dir()
                .ok_or_else(|| protocol_error("cannot resolve Codex home"))?
                .join(".codex"),
        };
        let home = home.canonicalize().map_err(|error| {
            protocol_error(format!(
                "Codex home {} is unavailable: {error}",
                home.display()
            ))
        })?;
        // Codex 0.156.1 app_server_control_socket_path contract.
        Ok(Self {
            socket: home.join("app-server-control/app-server-control.sock"),
            home,
        })
    }
}

/// Identity of the process actually serving this connection, never of a CLI or proxy.
/// This is an instance fence, not evidence that a Session or Turn has exited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DaemonIdentity {
    pub instance_id: String,
    process: pontia_runtime::ProcessFingerprint,
}

impl DaemonIdentity {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn capture(stream: &UnixStream) -> Result<Self> {
        let process = peer_process(stream)?;
        Ok(Self {
            instance_id: format!(
                "codex_{}_{}_{}",
                process.boot_time_seconds, process.pid, process.start_time_seconds
            ),
            process,
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn capture(_stream: &UnixStream) -> Result<Self> {
        Err(protocol_error(
            "daemon instance verification requires Linux or macOS",
        ))
    }
}

pub(crate) fn peer_process(stream: &UnixStream) -> Result<pontia_runtime::ProcessFingerprint> {
    let credentials = stream.peer_cred().map_err(protocol_error)?;
    let pid = credentials
        .pid()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| protocol_error("socket peer has no process identity"))?;
    // The daemon is a per-user service. Do not bind another user's process.
    if credentials.uid() != unsafe { libc::geteuid() } {
        return Err(protocol_error("socket peer belongs to another user"));
    }
    pontia_runtime::ProcessFingerprint::capture(pid as u32)
        .ok_or_else(|| protocol_error("cannot identify Codex socket peer"))
}

pub(super) async fn connect_socket(path: &std::path::Path) -> std::io::Result<UnixStream> {
    // Codex can publish a long endpoint as a symlink to a short socket path.
    let resolved;
    let path = if path.as_os_str().as_encoded_bytes().len() > 103 {
        resolved = std::fs::canonicalize(path)?;
        resolved.as_path()
    } else {
        path
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), UnixStream::connect(path)).await?
}

// Keep the startup lock until this process completes initialize. On cancellation
// or handshake failure, reap the unpublished child before releasing the lock.
pub(super) struct StartingServer {
    child: Option<std::process::Child>,
    _lock: std::fs::File,
}

pub(super) fn live_owners(
    socket: &std::path::Path,
) -> Result<Vec<pontia_runtime::ProcessFingerprint>> {
    use pontia_runtime::{ProcessFingerprint, ProcessObservation};
    let bytes = match std::fs::read(socket.with_extension("owner")) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut live = Vec::new();
    for process in serde_json::from_slice::<Vec<ProcessFingerprint>>(&bytes)? {
        match process.observe() {
            ProcessObservation::Alive => live.push(process),
            ProcessObservation::Exited => {}
            ProcessObservation::Unknown => {
                return Err(protocol_error("cannot verify Codex server identity"));
            }
        }
    }
    Ok(live)
}

impl StartingServer {
    pub(super) fn ready(
        mut self,
        socket: &std::path::Path,
        identity: &DaemonIdentity,
    ) -> Result<()> {
        let child = self.child.as_ref().unwrap();
        let peer = &identity.process;
        if !peer.is_in_process_tree(child.id()) {
            return Err(protocol_error(
                "background socket peer does not belong to the spawned Codex process",
            ));
        }
        // An unlinked socket can be rebound while its previous server still lives.
        let mut owned = live_owners(socket)?;
        owned.push(peer.clone());
        let temp = tempfile::NamedTempFile::new_in(socket.parent().unwrap())?;
        serde_json::to_writer(temp.as_file(), &owned)?;
        temp.persist(socket.with_extension("owner"))
            .map_err(|error| error.error)?;

        pontia_runtime::reap_detached_child(self.child.take().unwrap());
        Ok(())
    }
}

impl Drop for StartingServer {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub(super) async fn connect_background(
    endpoint: &Endpoint,
) -> Result<(UnixStream, Option<StartingServer>)> {
    use std::{
        fs::{OpenOptions, TryLockError},
        io::ErrorKind,
        os::unix::fs::FileTypeExt,
        process::Stdio,
        time::Duration,
    };
    use tokio::time::{Instant, sleep};

    async fn probe(socket: &std::path::Path) -> Result<Option<UnixStream>> {
        match connect_socket(socket).await {
            Ok(stream) => Ok(Some(stream)),
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::ConnectionRefused
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(protocol_error(error)),
        }
    }
    let socket = &endpoint.socket;
    let directory = socket.parent().expect("background socket directory");
    pontia_core::private_fs::directory(directory).map_err(protocol_error)?;
    let lock_path = socket.with_extension("lock");
    if let Some(stream) = probe(socket).await? {
        // A socket is visible before its creator has finished validating the server.
        match std::fs::File::open(&lock_path) {
            Ok(lock) => match lock.try_lock_shared() {
                Ok(()) => return Ok((stream, None)),
                Err(TryLockError::WouldBlock) => {}
                Err(error) => return Err(protocol_error(error)),
            },
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok((stream, None)),
            Err(error) => return Err(protocol_error(error)),
        }
    }
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock_path)
        .map_err(protocol_error)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                sleep(Duration::from_millis(50)).await
            }
            Err(error) => {
                return Err(protocol_error(format!(
                    "background app-server startup lock: {error}"
                )));
            }
        }
    }
    if let Some(stream) = probe(socket).await? {
        return Ok((stream, None));
    }
    match std::fs::symlink_metadata(socket) {
        Ok(metadata) if metadata.file_type().is_socket() || metadata.is_symlink() => {
            std::fs::remove_file(socket).map_err(protocol_error)?
        }
        Ok(_) => {
            return Err(protocol_error(
                "background app-server endpoint is not a socket",
            ));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(protocol_error(error)),
    }
    let executable =
        crate::setup::resolve_executable(pontia_runtime::executable::inherited_search_path(None))
            .map_err(protocol_error)?;
    let log = pontia_core::private_fs::append_log(&directory.parent().unwrap().join("runtime.log"))
        .map_err(protocol_error)?;
    let mut command =
        pontia_runtime::detached_command(&executable, "Pontia background Codex app-server")
            .map_err(protocol_error)?;
    command
        .args(["app-server", "--listen"])
        .arg(format!("unix://{}", socket.display()))
        .env("CODEX_HOME", &endpoint.home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    let mut starting = StartingServer {
        child: Some(command.spawn().map_err(protocol_error)?),
        _lock: lock,
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = starting
            .child
            .as_mut()
            .unwrap()
            .try_wait()
            .map_err(protocol_error)?
        {
            return Err(protocol_error(format!(
                "background app-server exited during startup: {status}"
            )));
        }
        if let Some(stream) = probe(socket).await? {
            return Ok((stream, Some(starting)));
        }
        if Instant::now() >= deadline {
            return Err(protocol_error("background app-server socket timed out"));
        }
        sleep(Duration::from_millis(50)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{CodexRuntime, CodexServerTarget};
    use pontia_runtime::{GenericRuntimeManager, ProcessFingerprint};

    #[test]
    fn abandoned_start_reaps_the_child_before_unlocking() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("startup.lock");
        let lock = std::fs::File::create(&path).unwrap();
        lock.lock().unwrap();
        let child = std::process::Command::new("sleep")
            .arg("10")
            .spawn()
            .unwrap();
        let pid = child.id();
        let pending = StartingServer {
            child: Some(child),
            _lock: lock,
        };
        let other = std::fs::File::open(path).unwrap();
        assert!(matches!(
            other.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(pending);
        other.try_lock().unwrap();
        assert_eq!(
            unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }

    #[tokio::test]
    #[ignore = "requires the native codex executable"]
    async fn concurrent_background_launch_reuses_one_daemon_across_control_restarts() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().canonicalize().unwrap();
        let directory = home.join("state/codex");
        std::fs::create_dir_all(&directory).unwrap();
        let endpoint = Endpoint {
            socket: directory.join("background-app-server.sock"),
            home: home.clone(),
        };
        // Leave a refused socket from a previous instance.
        drop(std::os::unix::net::UnixListener::bind(&endpoint.socket).unwrap());
        let connect = || {
            super::super::connect_daemon(
                &endpoint,
                CodexServerTarget::Background,
                Default::default(),
            )
        };
        let (first, second) = tokio::join!(connect(), connect());
        let first = first.unwrap();
        let pid = connect_socket(&endpoint.socket)
            .await
            .unwrap()
            .peer_cred()
            .unwrap()
            .pid()
            .unwrap();
        struct Cleanup(ProcessFingerprint);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = GenericRuntimeManager.terminate_process(&self.0);
            }
        }
        let original = Cleanup(ProcessFingerprint::capture(pid as u32).unwrap());
        let recorded = super::super::background_server(&home).await.unwrap();
        assert_eq!(recorded.owned.as_slice(), std::slice::from_ref(&original.0));
        assert!(recorded.unverified_listener.is_none());
        let second = second.unwrap();
        assert_eq!(second.identity, first.identity);
        second.close().await;
        assert_eq!(unsafe { libc::getsid(pid) }, pid);
        first.close().await;
        let user_dir = home.join("app-server-control");
        std::fs::create_dir_all(&user_dir).unwrap();
        let user_socket = user_dir.join("app-server-control.sock");
        let mut user_server = tokio::process::Command::new("codex")
            .args(["app-server", "--listen"])
            .arg(format!("unix://{}", user_socket.display()))
            .env("CODEX_HOME", &home)
            .current_dir(&home)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            while !user_socket.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let user = CodexRuntime::install_for_test(&home, user_socket)
            .await
            .unwrap();
        let connect = || {
            CodexRuntime::connect(
                home.clone(),
                CodexServerTarget::Background,
                Endpoint {
                    socket: endpoint.socket.clone(),
                    home: home.clone(),
                },
            )
        };
        let initial = connect().await.unwrap();
        let connection = initial.connection().await.unwrap();
        let identity = connection.identity.clone();
        let user_connection = user.connection().await.unwrap();
        assert_ne!(user_connection.identity, identity);
        let thread = connection
            .call(
                "thread/start",
                serde_json::json!({"cwd":home,"ephemeral":true}),
            )
            .await
            .unwrap();
        let id = thread["thread"]["id"].as_str().unwrap();
        let loaded = |response: serde_json::Value| {
            response["data"]
                .as_array()
                .unwrap()
                .iter()
                .any(|thread| thread == id)
        };
        assert!(loaded(
            connection
                .call("thread/loaded/list", serde_json::json!({}))
                .await
                .unwrap()
        ));
        assert!(!loaded(
            user_connection
                .call("thread/loaded/list", serde_json::json!({}))
                .await
                .unwrap()
        ));
        user_server.kill().await.unwrap();
        assert!(loaded(
            connection
                .call("thread/loaded/list", serde_json::json!({}))
                .await
                .unwrap()
        ));
        CodexRuntime::shutdown(&home).await;
        let recovered = connect().await.unwrap();
        let connection = recovered.connection().await.unwrap();
        assert_eq!(connection.identity, identity);
        assert!(loaded(
            connection
                .call("thread/loaded/list", serde_json::json!({}))
                .await
                .unwrap()
        ));
        assert_eq!(recovered.codex_home(), home);
        CodexRuntime::shutdown(&home).await;
        // Moving a Pontia home retains the ownership record while the old
        // app-server continues listening at its original native socket.
        let moved = home.join("moved");
        std::fs::create_dir(&moved).unwrap();
        std::fs::rename(home.join("state"), moved.join("state")).unwrap();
        let socket = moved.join("state/codex/background-app-server.sock");
        std::fs::remove_file(&socket).unwrap();
        let replacement = CodexRuntime::connect(
            moved.clone(),
            CodexServerTarget::Background,
            Endpoint {
                socket,
                home: home.clone(),
            },
        )
        .await
        .unwrap();
        let replacement = Cleanup(
            replacement
                .connection()
                .await
                .unwrap()
                .identity
                .process
                .clone(),
        );
        assert_ne!(replacement.0, original.0);
        let recorded = super::super::background_server(&moved).await.unwrap();
        assert_eq!(recorded.owned, [original.0.clone(), replacement.0.clone()]);
        assert!(recorded.unverified_listener.is_none());
        CodexRuntime::shutdown(&moved).await;
    }
}

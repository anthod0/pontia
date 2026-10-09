use super::protocol::protocol_error;
use pontia_core::Result;
use std::path::PathBuf;
use tokio::net::UnixStream;

pub(super) struct Endpoint {
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
}

impl DaemonIdentity {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub fn capture(stream: &UnixStream) -> Result<Self> {
        let credentials = stream.peer_cred().map_err(protocol_error)?;
        let pid = credentials
            .pid()
            .filter(|pid| *pid > 0)
            .ok_or_else(|| protocol_error("daemon peer has no process identity"))?;
        // The daemon is a per-user service. Do not bind another user's process.
        if credentials.uid() != unsafe { libc::geteuid() } {
            return Err(protocol_error("daemon belongs to another user"));
        }
        let identity = pontia_runtime::process::process_identity(pid as u32)
            .ok_or_else(|| protocol_error("cannot read daemon process start time"))?;
        let boot_time = pontia_runtime::process::system_boot_time_seconds()
            .ok_or_else(|| protocol_error("cannot read system boot time"))?;
        Ok(Self {
            instance_id: format!(
                "codex_{}_{}_{}",
                boot_time, identity.pid, identity.start_time_seconds
            ),
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn capture(_stream: &UnixStream) -> Result<Self> {
        Err(protocol_error(
            "daemon instance verification requires Linux or macOS",
        ))
    }
}

use super::{CodexRuntime, TuiTarget, protocol::protocol_error};
use pontia_core::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, TmuxProcessFingerprint};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command, sync::Arc, time::Duration};

// Codex 0.156.1 tui/src/app/reconnect.rs gives native reconnection 120 seconds.
pub(super) const RECONNECT_WINDOW: Duration = Duration::from_secs(125);
const ATTACH_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct TuiProcess {
    pub socket: String,
    pub pane: String,
    pub fingerprint: TmuxProcessFingerprint,
    pub argv: Vec<String>,
}

impl TuiProcess {
    pub fn peer_identity(&self) -> String {
        format!(
            "codex_{}_{}_{}",
            self.fingerprint.boot_id,
            self.fingerprint.agent_pid,
            self.fingerprint.agent_start_time_ticks
        )
    }

    pub(super) fn is_alive(&self) -> bool {
        GenericRuntimeManager.validate_tmux_process_fingerprint(
            &self.socket,
            &self.pane,
            &self.fingerprint,
        ) && process_args(self.fingerprint.agent_pid).as_ref() == Some(&self.argv)
    }

    async fn stop(&self) -> Result<()> {
        use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
        // Pin the process before validating it, so PID reuse cannot redirect the signal.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, self.fingerprint.agent_pid, 0) };
        if fd < 0 {
            return Err(protocol_error(
                "cannot pin the owned TUI process for recovery",
            ));
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        if !self.is_alive() {
            return Err(Error::StateConflict(
                "TUI process ownership changed; recovery was cancelled".into(),
            ));
        }
        if unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                libc::SIGTERM,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        tokio::time::timeout(Duration::from_secs(5), async {
            while GenericRuntimeManager.is_tmux_pane_alive(&self.socket, &self.pane) {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .map_err(|_| protocol_error("owned TUI did not exit after recovery was requested"))?;
        Ok(())
    }
}

fn process_args(pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    bytes
        .split(|byte| *byte == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| String::from_utf8(arg.to_vec()).ok())
        .collect()
}

impl CodexRuntime {
    pub(super) fn tui_file(&self, owner: &str, kind: &str) -> std::path::PathBuf {
        self.root
            .join("state/codex")
            .join(format!("{kind}-{owner}.json"))
    }

    pub(super) fn save_tui<T: Serialize>(&self, owner: &str, kind: &str, value: &T) -> Result<()> {
        let path = self.tui_file(owner, kind);
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, serde_json::to_vec(value)?)?;
        std::fs::rename(temporary, path)?;
        Ok(())
    }

    pub(super) fn saved_tui<T: for<'a> Deserialize<'a>>(
        &self,
        owner: &str,
        kind: &str,
    ) -> Result<Option<T>> {
        match std::fs::read(self.tui_file(owner, kind)) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) fn saved_target(&self, owner: &str) -> Result<Option<TuiTarget>> {
        self.saved_tui(owner, "target")
    }

    pub(crate) async fn connected_tui_owner(&self, thread: &str) -> Result<Option<String>> {
        let targets = self.tui_targets.lock().await;
        let mut owners: Vec<_> = targets
            .values()
            .filter(|target| target.connected && target.thread["id"] == thread)
            .collect();
        owners.sort_by_key(|target| &target.owner_session_id);
        for target in owners {
            if let Some(process) =
                self.saved_tui::<TuiProcess>(&target.owner_session_id, "process")?
                && process.peer_identity() == target.peer_identity
                && process.is_alive()
            {
                return Ok(Some(target.owner_session_id.clone()));
            }
        }
        Ok(None)
    }

    pub(crate) async fn open_tui(
        self: &Arc<Self>,
        owner: &str,
        thread: &str,
        cwd: &Path,
    ) -> Result<TuiTarget> {
        let endpoint = self.gateway(owner).await?;
        let _current = self.current_guard().await?;
        let process: Option<TuiProcess> = self.saved_tui(owner, "process")?;
        if let Some(process) = &process {
            if process.is_alive() {
                if let Some(target) = self.attached_tui(owner, thread, process).await? {
                    return Ok(target);
                }
                // A live connection or native reconnect window is never a licence to kill.
                let confirmed = self
                    .saved_target(owner)?
                    .filter(|target| target.peer_identity == process.peer_identity());
                if !confirmed.is_some_and(|target| target.thread["id"] == thread) {
                    return Err(Error::StateConflict(
                        "TUI target is unconfirmed or has changed; recovery was cancelled".into(),
                    ));
                }
                self.retire_tui_connection(owner, &process.peer_identity())
                    .await?;
                process.stop().await?;
            } else if GenericRuntimeManager.is_tmux_pane_alive(&process.socket, &process.pane) {
                return Err(Error::StateConflict(
                    "TUI pane is now occupied by an unverified process".into(),
                ));
            }
        }
        let quote = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let name = format!(
            "pontia_codex_{}",
            pontia_core::ids::new_runtime_instance_id()
        );
        let command = format!(
            "exec env CODEX_HOME={} {} resume --remote {} {}",
            quote(&self.connection.codex_home.to_string_lossy()),
            quote(&self.tui_command),
            quote(&endpoint),
            quote(thread)
        );
        let socket = self.root.join("state/codex/tmux.sock");
        let output = Command::new("tmux")
            .args([
                "-f",
                "/dev/null",
                "-S",
                socket
                    .to_str()
                    .ok_or_else(|| protocol_error("invalid TUI socket path"))?,
                "new-session",
                "-d",
                "-P",
                "-F",
                "#{pane_id}",
                "-s",
                &name,
                "-c",
                &cwd.to_string_lossy(),
                &command,
            ])
            .output()?;
        if !output.status.success() {
            return Err(protocol_error(format!(
                "could not launch Codex TUI: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        let socket_path = socket.to_string_lossy().into_owned();
        let pane_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let process = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let output = Command::new("tmux")
                    .args([
                        "-S",
                        &socket_path,
                        "display-message",
                        "-p",
                        "-t",
                        &pane_id,
                        "#{pane_pid}",
                    ])
                    .output()?;
                if let Ok(pid) = String::from_utf8_lossy(&output.stdout)
                    .trim()
                    .parse::<u32>()
                    && let Some(argv) = process_args(pid)
                    && argv
                        .windows(2)
                        .any(|args| args == ["--remote", endpoint.as_str()])
                    && let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm"))
                    && let Some(fingerprint) = GenericRuntimeManager
                        .capture_tmux_process_fingerprint(&socket_path, &pane_id, &[comm.trim()])
                    && fingerprint.agent_pid == pid
                {
                    break Ok::<_, Error>(TuiProcess {
                        socket: socket_path.clone(),
                        pane: pane_id.clone(),
                        fingerprint,
                        argv,
                    });
                }
                if !GenericRuntimeManager.is_tmux_pane_alive(&socket_path, &pane_id) {
                    break Err(protocol_error("Codex TUI exited during startup"));
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .map_err(|_| protocol_error("cannot verify the launched Codex TUI process"))??;
        self.save_tui(owner, "process", &process)?;
        tokio::time::timeout(ATTACH_TIMEOUT, async {
            loop {
                if let Some(target) = self.attached_tui(owner, thread, &process).await? {
                    return Ok(target);
                }
                if let Some(error) = self.tui_attachment_error(owner, &process).await {
                    return Err(Error::StateConflict(error));
                }
                if !process.is_alive() {
                    return Err(protocol_error(
                        "Codex TUI exited before attaching to the thread",
                    ));
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .map_err(|_| {
            protocol_error(
                "TUI has not confirmed thread attachment; inspect the terminal and retry Open TUI",
            )
        })?
    }

    async fn attached_tui(
        &self,
        owner: &str,
        thread: &str,
        process: &TuiProcess,
    ) -> Result<Option<TuiTarget>> {
        let targets = self.tui_targets.lock().await;
        if let Some(target) = targets.get(owner)
            && target.peer_identity == process.peer_identity()
        {
            if !target.connected {
                return Ok(None);
            }
            if target.thread["id"] != thread {
                return Err(Error::StateConflict("This TUI is displaying another Codex thread; use /resume in that TUI to switch back".into()));
            }
            return Ok(Some(target.clone()));
        }
        Ok(None)
    }

    async fn tui_attachment_error(&self, owner: &str, process: &TuiProcess) -> Option<String> {
        self.tui_targets
            .lock()
            .await
            .get(owner)
            .filter(|target| {
                target.peer_identity == process.peer_identity() && target.thread.is_null()
            })
            .and_then(|target| target.error.clone())
    }

    pub(crate) fn tui_pane(&self, owner: &str) -> Result<(String, String)> {
        let process: TuiProcess = self
            .saved_tui(owner, "process")?
            .ok_or_else(|| protocol_error("owned Codex TUI process is missing"))?;
        Ok((process.socket, process.pane))
    }
}

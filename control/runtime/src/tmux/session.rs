use std::{
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};

use pontia_core::error::{Error, Result};

use crate::RuntimeStartRequest;

use super::identifier::{sanitize_tmux_identifier, short_session_id};

pub(crate) fn spawn_tmux_session(
    tmux_session: &str,
    workspace: &Path,
    launch_command: &str,
) -> std::io::Result<ExitStatus> {
    let command = || {
        Command::new("tmux")
            .args([
                "new-session",
                "-d",
                "-s",
                tmux_session,
                "-c",
                &workspace.display().to_string(),
                launch_command,
            ])
            .status()
    };

    let first = command()?;
    if first.success() {
        return Ok(first);
    }

    thread::sleep(Duration::from_millis(50));
    command()
}

pub(crate) fn interrupt_session(socket_path: &str, pane_id: &str) -> Result<()> {
    if !super::pane::is_pane_alive(socket_path, pane_id) {
        return Err(Error::Domain(format!(
            "tmux runtime pane {pane_id} is not alive"
        )));
    }
    let status = Command::new("tmux")
        .args(["-S", socket_path, "send-keys", "-t", pane_id, "Escape"])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux runtime interrupt failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux runtime interrupt failed with status {status}"
        )));
    }
    Ok(())
}

pub(crate) fn terminate_session(runtime_handle: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["kill-session", "-t", runtime_handle])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux runtime terminate failed: {err}")))?;
    if status.success() || !is_alive(runtime_handle) {
        Ok(())
    } else {
        Err(Error::Domain(format!(
            "tmux runtime terminate failed with status {status}"
        )))
    }
}

pub(crate) fn is_alive(runtime_handle: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", runtime_handle])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(crate) fn tmux_session_name(request: &RuntimeStartRequest) -> String {
    let short_id = short_session_id(&request.session_id);
    let handle = request
        .handle
        .as_deref()
        .map(|value| value.trim_start_matches('@'))
        .filter(|value| !value.is_empty())
        .map(sanitize_tmux_identifier);
    let role = request
        .role
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(sanitize_tmux_identifier);
    let mut parts = vec!["pontia".to_string()];
    if let Some(handle) = handle {
        parts.push(handle);
    }
    if let Some(role) = role {
        parts.push(role);
    }
    if parts.len() == 1 {
        return format!("pontia_{}", sanitize_tmux_identifier(&request.session_id));
    }
    parts.push(short_id);
    parts.join("_")
}

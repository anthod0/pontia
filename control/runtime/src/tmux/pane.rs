use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use pontia_core::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TmuxPaneBinding {
    pub(crate) socket_path: String,
    pub(crate) pane_id: String,
}

pub(crate) fn run_launch_command_in_pane(
    socket_path: &str,
    pane_id: &str,
    launch_command: &str,
) -> Result<()> {
    if !is_pane_alive(socket_path, pane_id) {
        return Err(Error::Domain(format!(
            "tmux runtime pane {pane_id} is not alive"
        )));
    }
    let status = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "send-keys",
            "-t",
            pane_id,
            launch_command,
            "Enter",
        ])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux pane script start failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux pane script start failed with status {status}"
        )));
    }
    Ok(())
}

pub(crate) fn kill_pane(socket_path: &str, pane_id: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["-S", socket_path, "kill-pane", "-t", pane_id])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux kill-pane failed: {err}")))?;
    if status.success() || !is_pane_alive(socket_path, pane_id) {
        Ok(())
    } else {
        Err(Error::Domain(format!(
            "tmux kill-pane failed with status {status}"
        )))
    }
}

pub(crate) fn send_keys(socket_path: &str, pane_id: &str, keys: &[&str]) -> Result<()> {
    if !is_pane_alive(socket_path, pane_id) {
        return Err(Error::Domain(format!(
            "tmux runtime pane {pane_id} is not alive"
        )));
    }
    if keys.is_empty() {
        return Ok(());
    }

    for key in keys {
        let status = Command::new("tmux")
            .args(["-S", socket_path, "send-keys", "-t", pane_id, key])
            .stderr(Stdio::null())
            .status()
            .map_err(|err| Error::Domain(format!("tmux send-keys failed: {err}")))?;
        if !status.success() {
            return Err(Error::Domain(format!(
                "tmux send-keys failed with status {status}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

pub(crate) fn is_pane_alive(socket_path: &str, pane_id: &str) -> bool {
    let output = Command::new("tmux")
        .args(["-S", socket_path, "list-panes", "-a", "-F", "#{pane_id}"])
        .stderr(Stdio::null())
        .output();
    output.is_ok_and(|output| {
        output.status.success()
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line == pane_id)
    })
}

pub(crate) fn pane_binding(runtime_handle: &str) -> Option<TmuxPaneBinding> {
    let output = Command::new("tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            runtime_handle,
            "#{socket_path}\t#{pane_id}",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let mut parts = text.trim().split('\t');
    let socket_path = parts.next()?.to_string();
    let pane_id = parts.next()?.to_string();
    if socket_path.is_empty() || pane_id.is_empty() {
        return None;
    }
    Some(TmuxPaneBinding {
        socket_path,
        pane_id,
    })
}

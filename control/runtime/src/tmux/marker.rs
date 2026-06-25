use std::process::{Command, Stdio};

use pontia_core::error::{Error, Result};

use super::pane::is_pane_alive;

const PONTIA_SESSION_MARKER: &str = "@pontia_session_id";
const PONTIA_RUNTIME_INSTANCE_MARKER: &str = "@pontia_runtime_instance_id";
const REUSABLE_SHELL_COMMANDS: &[&str] = &["sh", "bash", "zsh", "fish"];

pub(crate) fn mark_pontia_pane(
    socket_path: &str,
    pane_id: &str,
    session_id: &str,
    runtime_instance_id: &str,
) -> Result<()> {
    set_pane_option(socket_path, pane_id, PONTIA_SESSION_MARKER, session_id)?;
    set_pane_option(
        socket_path,
        pane_id,
        PONTIA_RUNTIME_INSTANCE_MARKER,
        runtime_instance_id,
    )
}

pub(crate) fn is_reusable_pontia_shell_pane(
    socket_path: &str,
    pane_id: &str,
    session_id: &str,
) -> bool {
    if !is_pane_alive(socket_path, pane_id) {
        return false;
    }
    if pane_option(socket_path, pane_id, PONTIA_SESSION_MARKER).as_deref() != Some(session_id) {
        return false;
    }
    pane_current_command(socket_path, pane_id).is_some_and(|command| {
        REUSABLE_SHELL_COMMANDS
            .iter()
            .any(|shell| command == *shell)
    })
}

fn set_pane_option(socket_path: &str, pane_id: &str, option: &str, value: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "set-option",
            "-p",
            "-t",
            pane_id,
            option,
            value,
        ])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux pane marker failed: {err}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Domain(format!(
            "tmux pane marker failed with status {status}"
        )))
    }
}

fn pane_option(socket_path: &str, pane_id: &str, option: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "show-options",
            "-p",
            "-v",
            "-t",
            pane_id,
            option,
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn pane_current_command(socket_path: &str, pane_id: &str) -> Option<String> {
    let output = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "display-message",
            "-p",
            "-t",
            pane_id,
            "#{pane_current_command}",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_string())
        .filter(|value| !value.is_empty())
}

use std::{
    io::Write,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};

use crate::error::{Error, Result};

use super::{AgentInput, RuntimeStartRequest, shell_quote};

pub(super) fn spawn_tmux_session(
    tmux_session: &str,
    workspace: &Path,
    script_path: &Path,
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
                &format!("sh {}", shell_quote(&script_path.display().to_string())),
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

pub(super) fn dispatch_tui_turn(
    runtime_ref: &str,
    client_type: &str,
    input: &AgentInput,
) -> Result<()> {
    if !is_alive(runtime_ref) {
        return Err(Error::Domain(format!(
            "{client_type} runtime {runtime_ref} is not alive"
        )));
    }

    let buffer_name = format!("pilotfy_{}", sanitize_tmux_identifier(&input.turn_id));
    let mut child = Command::new("tmux")
        .args(["load-buffer", "-b", &buffer_name, "-"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|err| Error::Domain(format!("tmux dispatch buffer failed: {err}")))?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| Error::Domain("tmux dispatch buffer stdin unavailable".to_string()))?;
        stdin.write_all(input.input.as_bytes())?;
    }
    let status = child
        .wait()
        .map_err(|err| Error::Domain(format!("tmux dispatch buffer failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux dispatch buffer failed with status {status}"
        )));
    }

    let status = Command::new("tmux")
        .args(["paste-buffer", "-t", runtime_ref, "-b", &buffer_name])
        .status()
        .map_err(|err| Error::Domain(format!("tmux dispatch paste failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux dispatch paste failed with status {status}"
        )));
    }

    let status = Command::new("tmux")
        .args(["send-keys", "-t", runtime_ref, "Enter"])
        .status()
        .map_err(|err| Error::Domain(format!("tmux dispatch submit failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux dispatch submit failed with status {status}"
        )));
    }

    let _ = Command::new("tmux")
        .args(["delete-buffer", "-b", &buffer_name])
        .status();
    Ok(())
}

pub(super) fn interrupt_session(runtime_ref: &str) -> Result<()> {
    if !is_alive(runtime_ref) {
        return Err(Error::Domain(format!(
            "tmux runtime {runtime_ref} is not alive"
        )));
    }
    let status = Command::new("tmux")
        .args(["send-keys", "-t", runtime_ref, "Escape"])
        .status()
        .map_err(|err| Error::Domain(format!("tmux runtime interrupt failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux runtime interrupt failed with status {status}"
        )));
    }
    Ok(())
}

pub(super) fn terminate_session(runtime_ref: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["kill-session", "-t", runtime_ref])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux runtime terminate failed: {err}")))?;
    if status.success() || !is_alive(runtime_ref) {
        Ok(())
    } else {
        Err(Error::Domain(format!(
            "tmux runtime terminate failed with status {status}"
        )))
    }
}

pub(super) fn is_alive(runtime_ref: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", runtime_ref])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(super) fn tmux_session_name(request: &RuntimeStartRequest) -> String {
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
    let mut parts = vec!["pilotfy".to_string()];
    if let Some(handle) = handle {
        parts.push(handle);
    }
    if let Some(role) = role {
        parts.push(role);
    }
    if parts.len() == 1 {
        return format!("pilotfy_{}", sanitize_tmux_identifier(&request.session_id));
    }
    parts.push(short_id);
    parts.join("_")
}

fn short_session_id(session_id: &str) -> String {
    let id_body = session_id.rsplit('_').next().unwrap_or(session_id);
    let mut chars: Vec<char> = id_body.chars().rev().take(8).collect();
    chars.reverse();
    chars.into_iter().collect()
}

fn sanitize_tmux_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

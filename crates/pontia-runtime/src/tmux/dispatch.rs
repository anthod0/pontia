use std::{
    io::Write,
    process::{Command, Stdio},
};

use pontia_core::error::{Error, Result};

use crate::AgentInput;

use super::{identifier::sanitize_tmux_identifier, pane::is_pane_alive};

pub(crate) fn dispatch_tui_turn(
    socket_path: &str,
    pane_id: &str,
    client_type: &str,
    input: &AgentInput,
) -> Result<()> {
    if !is_pane_alive(socket_path, pane_id) {
        return Err(Error::Domain(format!(
            "{client_type} runtime pane {pane_id} is not alive"
        )));
    }

    let buffer_name = format!("pontia_{}", sanitize_tmux_identifier(&input.dispatch_id));
    let mut child = Command::new("tmux")
        .args(["-S", socket_path, "load-buffer", "-b", &buffer_name, "-"])
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
        .args([
            "-S",
            socket_path,
            "paste-buffer",
            "-p",
            "-r",
            "-t",
            pane_id,
            "-b",
            &buffer_name,
        ])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux dispatch paste failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux dispatch paste failed with status {status}"
        )));
    }

    let status = Command::new("tmux")
        .args(["-S", socket_path, "send-keys", "-t", pane_id, "Enter"])
        .stderr(Stdio::null())
        .status()
        .map_err(|err| Error::Domain(format!("tmux dispatch submit failed: {err}")))?;
    if !status.success() {
        return Err(Error::Domain(format!(
            "tmux dispatch submit failed with status {status}"
        )));
    }

    let _ = Command::new("tmux")
        .args(["-S", socket_path, "delete-buffer", "-b", &buffer_name])
        .status();
    Ok(())
}

use std::{
    io::Write,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};

use crate::error::{Error, Result};

use super::{AgentInput, RuntimeStartRequest, utils::shell_quote};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TmuxPaneBinding {
    pub(super) socket_path: String,
    pub(super) pane_id: String,
}

const PONTIA_SESSION_MARKER: &str = "@pontia_session_id";
const PONTIA_RUNTIME_INSTANCE_MARKER: &str = "@pontia_runtime_instance_id";
const REUSABLE_SHELL_COMMANDS: &[&str] = &["sh", "bash", "zsh", "fish"];

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

pub(super) fn run_script_in_pane(
    socket_path: &str,
    pane_id: &str,
    script_path: &Path,
) -> Result<()> {
    if !is_pane_alive(socket_path, pane_id) {
        return Err(Error::Domain(format!(
            "tmux runtime pane {pane_id} is not alive"
        )));
    }
    let command = format!("sh {}", shell_quote(&script_path.display().to_string()));
    let status = Command::new("tmux")
        .args([
            "-S",
            socket_path,
            "send-keys",
            "-t",
            pane_id,
            &command,
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

pub(super) fn mark_pontia_pane(
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

pub(super) fn is_reusable_pontia_shell_pane(
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

pub(super) fn dispatch_tui_turn(
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

    let buffer_name = format!("pontia_{}", sanitize_tmux_identifier(&input.turn_id));
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

pub(super) fn interrupt_session(socket_path: &str, pane_id: &str) -> Result<()> {
    if !is_pane_alive(socket_path, pane_id) {
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

pub(super) fn terminate_session(runtime_handle: &str) -> Result<()> {
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

pub(super) fn kill_pane(socket_path: &str, pane_id: &str) -> Result<()> {
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

pub(super) fn send_keys(socket_path: &str, pane_id: &str, keys: &[&str]) -> Result<()> {
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

pub(super) fn is_alive(runtime_handle: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", runtime_handle])
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub(super) fn is_pane_alive(socket_path: &str, pane_id: &str) -> bool {
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

pub(super) fn pane_binding(runtime_handle: &str) -> Option<TmuxPaneBinding> {
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

#[cfg(test)]
mod tests {
    use std::{fs, process::Stdio};

    use super::*;

    #[test]
    fn dispatch_tui_turn_targets_bound_pane_with_socket_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let output = temp.path().join("messages.log");
        let session = format!("pontia_test_pane_{}", std::process::id());
        let command = format!("cat > {}", output.display());
        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", &session, &command])
            .stderr(Stdio::null())
            .status()
            .expect("spawn tmux");
        assert!(status.success(), "tmux session should start");

        let socket_path = Command::new("tmux")
            .args(["display-message", "-p", "-t", &session, "#{socket_path}"])
            .output()
            .expect("query socket path");
        assert!(
            socket_path.status.success(),
            "socket path query should succeed"
        );
        let socket_path = String::from_utf8(socket_path.stdout)
            .expect("socket path utf8")
            .trim()
            .to_string();
        let pane_id = Command::new("tmux")
            .args(["display-message", "-p", "-t", &session, "#{pane_id}"])
            .output()
            .expect("query pane id");
        assert!(pane_id.status.success(), "pane id query should succeed");
        let pane_id = String::from_utf8(pane_id.stdout)
            .expect("pane id utf8")
            .trim()
            .to_string();

        let input = AgentInput {
            session_id: "session_pane".to_string(),
            turn_id: "turn_pane".to_string(),
            input: "pane-bound input".to_string(),
        };
        let result = dispatch_tui_turn(&socket_path, &pane_id, "pi", &input);

        result.expect("dispatch to bound pane");
        thread::sleep(Duration::from_millis(200));
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &session])
            .stderr(Stdio::null())
            .status();
        let content = fs::read_to_string(&output).expect("fake tui output");
        assert!(content.contains("pane-bound input"));
    }

    #[test]
    fn pane_marker_and_shell_command_allow_reuse_only_for_matching_pontia_shell_pane() {
        let session = format!("pontia_test_reuse_shell_{}", std::process::id());
        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", &session, "sh"])
            .stderr(Stdio::null())
            .status()
            .expect("spawn tmux");
        assert!(status.success(), "tmux session should start");

        let binding = pane_binding(&session).expect("pane binding");
        mark_pontia_pane(
            &binding.socket_path,
            &binding.pane_id,
            "session_reuse",
            "rtinst_reuse",
        )
        .expect("mark pontia pane");

        assert!(is_reusable_pontia_shell_pane(
            &binding.socket_path,
            &binding.pane_id,
            "session_reuse",
        ));
        assert!(!is_reusable_pontia_shell_pane(
            &binding.socket_path,
            &binding.pane_id,
            "other_session",
        ));

        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &session])
            .stderr(Stdio::null())
            .status();
    }

    #[test]
    fn pane_with_non_shell_foreground_command_is_not_reusable() {
        let session = format!("pontia_test_reuse_busy_{}", std::process::id());
        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", &session, "exec sleep 60"])
            .stderr(Stdio::null())
            .status()
            .expect("spawn tmux");
        assert!(status.success(), "tmux session should start");

        thread::sleep(Duration::from_millis(100));
        let binding = pane_binding(&session).expect("pane binding");
        mark_pontia_pane(
            &binding.socket_path,
            &binding.pane_id,
            "session_reuse",
            "rtinst_reuse",
        )
        .expect("mark pontia pane");

        assert!(!is_reusable_pontia_shell_pane(
            &binding.socket_path,
            &binding.pane_id,
            "session_reuse",
        ));

        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &session])
            .stderr(Stdio::null())
            .status();
    }

    #[test]
    fn dispatch_tui_turn_preserves_multiline_input_as_one_bracketed_paste() {
        let temp = tempfile::tempdir().expect("tempdir");
        let script = temp.path().join("fake-tui.py");
        let output = temp.path().join("messages.log");
        fs::write(
            &script,
            r#"import os
import sys

out_path = os.environ["OUT"]
sys.stdout.write("\033[?2004h")
sys.stdout.flush()

buffer = ""
in_paste = False

def submit():
    global buffer
    if buffer:
        with open(out_path, "a", encoding="utf-8") as out:
            out.write(buffer + "\n---MESSAGE---\n")
        buffer = ""

while True:
    chunk = os.read(0, 1024)
    if not chunk:
        break
    text = chunk.decode("utf-8", errors="replace")
    i = 0
    while i < len(text):
        if text.startswith("\033[200~", i):
            in_paste = True
            i += len("\033[200~")
            continue
        if text.startswith("\033[201~", i):
            in_paste = False
            i += len("\033[201~")
            continue
        ch = text[i]
        if ch == "\n" and not in_paste:
            submit()
        else:
            buffer += ch
        i += 1
"#,
        )
        .expect("write fake tui");

        let session = format!("pontia_test_multiline_{}", std::process::id());
        let command = format!("OUT={} python3 {}", output.display(), script.display());
        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", &session, &command])
            .stderr(Stdio::null())
            .status()
            .expect("spawn tmux");
        assert!(status.success(), "tmux session should start");

        thread::sleep(Duration::from_millis(300));
        let socket_path = Command::new("tmux")
            .args(["display-message", "-p", "-t", &session, "#{socket_path}"])
            .output()
            .expect("query socket path");
        assert!(
            socket_path.status.success(),
            "socket path query should succeed"
        );
        let socket_path = String::from_utf8(socket_path.stdout)
            .expect("socket path utf8")
            .trim()
            .to_string();
        let pane_id = Command::new("tmux")
            .args(["display-message", "-p", "-t", &session, "#{pane_id}"])
            .output()
            .expect("query pane id");
        assert!(pane_id.status.success(), "pane id query should succeed");
        let pane_id = String::from_utf8(pane_id.stdout)
            .expect("pane id utf8")
            .trim()
            .to_string();
        let input = AgentInput {
            session_id: "session_multiline".to_string(),
            turn_id: "turn_multiline".to_string(),
            input: "line one\nline two".to_string(),
        };
        let result = dispatch_tui_turn(&socket_path, &pane_id, "pi", &input);

        result.expect("dispatch multiline turn");
        for _ in 0..50 {
            if output.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let content = fs::read_to_string(&output).expect("fake tui output");

        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &session])
            .stderr(Stdio::null())
            .status();
        assert_eq!(content, "line one\nline two\n---MESSAGE---\n");
    }
}

use std::{
    io::Write,
    path::Path,
    process::{Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};

use crate::error::{Error, Result};

use super::{AgentInput, RuntimeStartRequest, utils::shell_quote};

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

    let buffer_name = format!("pontia_{}", sanitize_tmux_identifier(&input.turn_id));
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
        .args([
            "paste-buffer",
            "-p",
            "-r",
            "-t",
            runtime_ref,
            "-b",
            &buffer_name,
        ])
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
        let input = AgentInput {
            session_id: "session_multiline".to_string(),
            turn_id: "turn_multiline".to_string(),
            input: "line one\nline two".to_string(),
        };
        let result = dispatch_tui_turn(&session, "pi", &input);

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

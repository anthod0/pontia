use std::{
    fs,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use crate::AgentInput;

use super::super::dispatch_tui_turn;

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
        dispatch_id: "dispatch_pane".to_string(),
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
        dispatch_id: "dispatch_multiline".to_string(),
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

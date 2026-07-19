mod dispatch;
mod identifier;
mod marker;
mod pane;
mod session;

pub(super) use dispatch::dispatch_tui_turn;
pub(super) use marker::{is_reusable_pontia_shell_pane, mark_pontia_pane};
pub(super) use pane::{
    TmuxPaneBinding, is_pane_alive, kill_pane, pane_binding, run_launch_command_in_pane, send_keys,
};
pub(super) use session::{
    interrupt_session, is_alive, spawn_tmux_session, terminate_session, tmux_session_name,
};

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::{Command, Stdio},
        thread,
        time::Duration,
    };

    use crate::{AgentInput, RuntimeStartRequest};

    use super::*;

    #[test]
    fn tmux_session_name_includes_workspace_name_and_short_session_id() {
        let name = tmux_session_name(&RuntimeStartRequest {
            session_id: "sess_1234567890abcdef".to_string(),
            client_type: "pi".to_string(),
            workspace: Some("/repo/ignored-path-name".to_string()),
            workspace_name: Some("Pontia App".to_string()),
            handle: Some("@main".to_string()),
            role: Some("coder".to_string()),
            agent_kind: None,
            start_command: None,
        });

        assert_eq!(name, "pontia_Pontia_App_main_coder_90abcdef");
    }

    #[test]
    fn tmux_session_name_falls_back_to_workspace_basename_and_never_uses_full_session_id() {
        let name = tmux_session_name(&RuntimeStartRequest {
            session_id: "sess_1234567890abcdef".to_string(),
            client_type: "pi".to_string(),
            workspace: Some("/repo/pontia".to_string()),
            workspace_name: None,
            handle: None,
            role: None,
            agent_kind: None,
            start_command: None,
        });

        assert_eq!(name, "pontia_pontia_90abcdef");
    }

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

        assert!(wait_for_reusable_pontia_shell_pane(
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

    fn wait_for_reusable_pontia_shell_pane(
        socket_path: &str,
        pane_id: &str,
        session_id: &str,
    ) -> bool {
        for _ in 0..50 {
            if is_reusable_pontia_shell_pane(socket_path, pane_id, session_id) {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        false
    }

    #[test]
    fn marked_shell_pane_with_foreground_child_process_is_not_reusable() {
        let session = format!("pontia_test_reuse_foreground_child_{}", std::process::id());
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
        send_keys(
            &binding.socket_path,
            &binding.pane_id,
            &["sleep 60", "Enter"],
        )
        .expect("start foreground child process");

        for _ in 0..50 {
            if !is_reusable_pontia_shell_pane(
                &binding.socket_path,
                &binding.pane_id,
                "session_reuse",
            ) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
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
}

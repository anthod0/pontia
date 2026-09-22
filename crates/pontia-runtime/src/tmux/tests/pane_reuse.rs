use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use super::super::{is_reusable_shell_pane, mark_pontia_pane, pane_binding, send_keys};

#[test]
fn shell_pane_is_reusable_while_pontia_markers_are_present() {
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

    assert!(wait_for_reusable_shell_pane(
        &binding.socket_path,
        &binding.pane_id,
    ));

    let _ = Command::new("tmux")
        .args(["kill-session", "-t", &session])
        .stderr(Stdio::null())
        .status();
}

fn wait_for_reusable_shell_pane(socket_path: &str, pane_id: &str) -> bool {
    for _ in 0..50 {
        if is_reusable_shell_pane(socket_path, pane_id) {
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
        if !is_reusable_shell_pane(&binding.socket_path, &binding.pane_id) {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!is_reusable_shell_pane(
        &binding.socket_path,
        &binding.pane_id,
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

    assert!(!is_reusable_shell_pane(
        &binding.socket_path,
        &binding.pane_id,
    ));

    let _ = Command::new("tmux")
        .args(["kill-session", "-t", &session])
        .stderr(Stdio::null())
        .status();
}

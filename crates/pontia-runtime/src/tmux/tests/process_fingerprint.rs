use std::{
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use super::super::{
    ProcessObservation, TmuxProcessFingerprint, capture_fingerprint, observe_fingerprint,
};

#[test]
fn legacy_tick_fingerprint_is_not_treated_as_epoch_seconds() {
    let legacy = serde_json::json!({
        "boot_id": "boot",
        "tmux_socket_path": "/tmp/tmux.sock",
        "tmux_pane_id": "%1",
        "pane_pid": 10,
        "pane_start_time_ticks": 20,
        "agent_pid": 11,
        "agent_start_time_ticks": 21,
        "agent_comm": "pi",
        "agent_argv0": "pi"
    });

    assert!(serde_json::from_value::<TmuxProcessFingerprint>(legacy).is_err());
}

struct TestServer(PathBuf);
impl Drop for TestServer {
    fn drop(&mut self) {
        let _ = Command::new("tmux")
            .arg("-S")
            .arg(&self.0)
            .arg("kill-server")
            .stderr(Stdio::null())
            .status();
    }
}

#[test]
fn process_fingerprint_tracks_the_exact_agent_process() {
    let root = tempfile::tempdir().unwrap();
    let server = TestServer(root.path().join("tmux.sock"));
    let socket = server.0.to_str().unwrap();
    let status = Command::new("tmux")
        .args([
            "-S",
            socket,
            "new-session",
            "-d",
            "-s",
            "fingerprint",
            "sleep 60",
        ])
        .stderr(Stdio::null())
        .status()
        .expect("spawn tmux");
    assert!(status.success(), "tmux session should start");
    let pane = Command::new("tmux")
        .args([
            "-S",
            socket,
            "display-message",
            "-p",
            "-t",
            "fingerprint",
            "#{pane_id}",
        ])
        .output()
        .unwrap();
    assert!(pane.status.success());
    let pane = String::from_utf8(pane.stdout).unwrap();
    let fingerprint = (0..50)
        .find_map(|_| {
            let fingerprint = capture_fingerprint(socket, pane.trim(), &["sleep"]);
            if fingerprint.is_none() {
                thread::sleep(Duration::from_millis(20));
            }
            fingerprint
        })
        .expect("capture sleep process fingerprint");
    assert!(fingerprint.pane_start_time_seconds >= fingerprint.boot_time_seconds);
    assert!(fingerprint.agent_start_time_seconds >= fingerprint.boot_time_seconds);
    assert_eq!(observe_fingerprint(&fingerprint), ProcessObservation::Alive);
    let mut unavailable_source = fingerprint.clone();
    unavailable_source.tmux_socket_path =
        root.path().join("unavailable.sock").display().to_string();
    assert_eq!(
        observe_fingerprint(&unavailable_source),
        ProcessObservation::Unknown
    );
    let status = Command::new("tmux")
        .args(["-S", socket, "kill-session", "-t", "fingerprint"])
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    // tmux's acknowledgement precedes the kernel reaping the pane processes.
    let observation = (0..50)
        .map(|_| {
            let state = observe_fingerprint(&fingerprint);
            if state != ProcessObservation::Exited {
                thread::sleep(Duration::from_millis(20));
            }
            state
        })
        .find(|state| *state == ProcessObservation::Exited);
    assert_eq!(observation, Some(ProcessObservation::Exited));
}

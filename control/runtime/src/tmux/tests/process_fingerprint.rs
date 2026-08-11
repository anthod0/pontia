use std::{
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use super::super::{capture_fingerprint, pane_binding, validate_fingerprint};

#[test]
fn process_fingerprint_tracks_the_exact_agent_process() {
    let session = format!("pontia_test_process_fingerprint_{}", std::process::id());
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", &session, "sleep 60"])
        .stderr(Stdio::null())
        .status()
        .expect("spawn tmux");
    assert!(status.success(), "tmux session should start");

    let binding = pane_binding(&session).expect("pane binding");
    let fingerprint = (0..50)
        .find_map(|_| {
            let fingerprint =
                capture_fingerprint(&binding.socket_path, &binding.pane_id, &["sleep"]);
            if fingerprint.is_none() {
                thread::sleep(Duration::from_millis(20));
            }
            fingerprint
        })
        .expect("capture sleep process fingerprint");
    assert!(validate_fingerprint(
        &binding.socket_path,
        &binding.pane_id,
        &fingerprint,
    ));

    let _ = Command::new("tmux")
        .args(["kill-session", "-t", &session])
        .stderr(Stdio::null())
        .status();
    assert!(!validate_fingerprint(
        &binding.socket_path,
        &binding.pane_id,
        &fingerprint,
    ));
}

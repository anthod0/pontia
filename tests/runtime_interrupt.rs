use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{Mutex, OnceLock},
};

use llmparty::runtime::GenericRuntimeManager;

fn path_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn interrupt_session_sends_two_ctrl_c_keys() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);

    let original_path = install_fake_tmux(tempdir.path(), &tmux_log, None);

    GenericRuntimeManager
        .interrupt_session("runtime-ref")
        .expect("interrupt session");

    restore_fake_tmux(original_path);

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let ctrl_c_sends = log
        .lines()
        .filter(|line| *line == "send-keys -t runtime-ref C-c")
        .count();
    assert_eq!(ctrl_c_sends, 2, "{log}");
}

#[test]
fn interrupt_session_succeeds_when_runtime_exits_after_first_ctrl_c() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let tmux_state = tempdir.path().join("tmux.state");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);

    let original_path = install_fake_tmux(tempdir.path(), &tmux_log, Some(&tmux_state));

    GenericRuntimeManager
        .interrupt_session("runtime-ref")
        .expect("interrupt session should tolerate runtime exiting after interrupt");

    restore_fake_tmux(original_path);

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let ctrl_c_sends = log
        .lines()
        .filter(|line| *line == "send-keys -t runtime-ref C-c")
        .count();
    assert_eq!(ctrl_c_sends, 1, "{log}");
}

fn install_fake_tmux(tempdir: &Path, tmux_log: &Path, tmux_state: Option<&Path>) -> String {
    let original_path = std::env::var("PATH").unwrap_or_default();
    unsafe {
        std::env::set_var("PATH", format!("{}:{original_path}", tempdir.display()));
        std::env::set_var("TMUX_LOG", tmux_log);
        if let Some(tmux_state) = tmux_state {
            std::env::set_var("TMUX_STATE", tmux_state);
        } else {
            std::env::remove_var("TMUX_STATE");
        }
    }
    original_path
}

fn restore_fake_tmux(original_path: String) {
    unsafe {
        std::env::set_var("PATH", original_path);
        std::env::remove_var("TMUX_LOG");
        std::env::remove_var("TMUX_STATE");
    }
}

fn write_fake_tmux(path: &Path) {
    std::fs::write(
        path,
        r#"#!/usr/bin/env sh
printf '%s\n' "$*" >> "$TMUX_LOG"
if [ "$1" = "has-session" ] && [ -n "${TMUX_STATE:-}" ] && [ -f "$TMUX_STATE" ]; then
  exit 1
fi
if [ "$1" = "send-keys" ] && [ -n "${TMUX_STATE:-}" ]; then
  : > "$TMUX_STATE"
fi
exit 0
"#,
    )
    .expect("write fake tmux");
    let mut permissions = std::fs::metadata(path)
        .expect("fake tmux metadata")
        .permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("fake tmux permissions");
}

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

    let original_path = std::env::var("PATH").unwrap_or_default();
    unsafe {
        std::env::set_var(
            "PATH",
            format!("{}:{original_path}", tempdir.path().display()),
        );
        std::env::set_var("TMUX_LOG", &tmux_log);
    }

    GenericRuntimeManager
        .interrupt_session("runtime-ref")
        .expect("interrupt session");

    unsafe {
        std::env::set_var("PATH", original_path);
        std::env::remove_var("TMUX_LOG");
    }

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let ctrl_c_sends = log
        .lines()
        .filter(|line| *line == "send-keys -t runtime-ref C-c")
        .count();
    assert_eq!(ctrl_c_sends, 2, "{log}");
}

fn write_fake_tmux(path: &Path) {
    std::fs::write(
        path,
        r#"#!/usr/bin/env sh
printf '%s\n' "$*" >> "$TMUX_LOG"
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

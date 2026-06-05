use std::{
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{Mutex, OnceLock},
};

use pilotfy::{
    agent_clients::InterruptBehavior,
    config::{RuntimeClientConfig, RuntimeConfig},
    runtime::{GenericRuntimeManager, RuntimeStartRequest, set_runtime_config},
};

fn path_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn interrupt_session_sends_escape_key() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);

    let original_path = install_fake_tmux(tempdir.path(), &tmux_log, None);

    GenericRuntimeManager
        .interrupt_session("runtime-ref", InterruptBehavior::TmuxInterrupt)
        .expect("interrupt session");

    restore_fake_tmux(original_path);

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let escape_sends = log
        .lines()
        .filter(|line| *line == "send-keys -t runtime-ref Escape")
        .count();
    assert_eq!(escape_sends, 1, "{log}");
}

#[test]
fn start_session_uses_configured_tui_command_when_env_is_absent() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);
    let original_path = install_fake_tmux(tempdir.path(), &tmux_log, None);
    unsafe {
        std::env::set_var("PILOTFY_DATA_DIR", tempdir.path().join("data"));
        std::env::remove_var("PILOTFY_PI_TUI_COMMAND");
    }
    set_runtime_config(RuntimeConfig {
        pi: RuntimeClientConfig {
            tui_command: Some("pi -e /configured/clients/pi".to_string()),
        },
        claude_code: RuntimeClientConfig::default(),
    });

    let result = GenericRuntimeManager
        .start_session(RuntimeStartRequest {
            session_id: "sess_configured".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().join("workspace").display().to_string()),
            handle: None,
            role: None,
            agent_kind: None,
        })
        .expect("start session");

    restore_fake_tmux(original_path);
    unsafe {
        std::env::remove_var("PILOTFY_DATA_DIR");
    }
    set_runtime_config(RuntimeConfig::default());

    let runtime_dir = result.metadata["runtime_dir"]
        .as_str()
        .expect("runtime dir");
    let script =
        std::fs::read_to_string(Path::new(runtime_dir).join("runtime.sh")).expect("runtime script");
    assert!(script.contains("pi -e /configured/clients/pi"), "{script}");
}

#[test]
fn start_session_prefers_env_tui_command_over_configured_command() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);
    let original_path = install_fake_tmux(tempdir.path(), &tmux_log, None);
    unsafe {
        std::env::set_var("PILOTFY_DATA_DIR", tempdir.path().join("data"));
        std::env::set_var("PILOTFY_PI_TUI_COMMAND", "pi from env");
    }
    set_runtime_config(RuntimeConfig {
        pi: RuntimeClientConfig {
            tui_command: Some("pi from config".to_string()),
        },
        claude_code: RuntimeClientConfig::default(),
    });

    let result = GenericRuntimeManager
        .start_session(RuntimeStartRequest {
            session_id: "sess_env_override".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().join("workspace-env").display().to_string()),
            handle: None,
            role: None,
            agent_kind: None,
        })
        .expect("start session");

    restore_fake_tmux(original_path);
    unsafe {
        std::env::remove_var("PILOTFY_DATA_DIR");
        std::env::remove_var("PILOTFY_PI_TUI_COMMAND");
    }
    set_runtime_config(RuntimeConfig::default());

    let runtime_dir = result.metadata["runtime_dir"]
        .as_str()
        .expect("runtime dir");
    let script =
        std::fs::read_to_string(Path::new(runtime_dir).join("runtime.sh")).expect("runtime script");
    assert!(script.contains("pi from env"), "{script}");
    assert!(!script.contains("pi from config"), "{script}");
}

#[test]
fn interrupt_session_succeeds_when_runtime_exits_after_escape() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let tmux_state = tempdir.path().join("tmux.state");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);

    let original_path = install_fake_tmux(tempdir.path(), &tmux_log, Some(&tmux_state));

    GenericRuntimeManager
        .interrupt_session("runtime-ref", InterruptBehavior::TmuxInterrupt)
        .expect("interrupt session should send escape successfully");

    restore_fake_tmux(original_path);

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let escape_sends = log
        .lines()
        .filter(|line| *line == "send-keys -t runtime-ref Escape")
        .count();
    assert_eq!(escape_sends, 1, "{log}");
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

use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{Mutex, OnceLock},
};

use pontia_agent_clients::InterruptBehavior;
use pontia_config::{RuntimeClientConfig, RuntimeConfig};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest, set_runtime_config};

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
        .interrupt_session("/tmp/tmux-test", "%42", InterruptBehavior::TmuxInterrupt)
        .expect("interrupt session");

    restore_fake_tmux(original_path);

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let escape_sends = log
        .lines()
        .filter(|line| *line == "-S /tmp/tmux-test send-keys -t %42 Escape")
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
        std::env::set_var("PONTIA_HOME", tempdir.path().join("pontia-home"));
        std::env::remove_var("PONTIA_PI_TUI_COMMAND");
    }
    set_runtime_config(RuntimeConfig {
        clients: HashMap::from([(
            "pi".to_string(),
            RuntimeClientConfig {
                tui_command: Some("pi --approve -e /configured/clients/pi".to_string()),
            },
        )]),
    });

    GenericRuntimeManager
        .start_session(RuntimeStartRequest {
            session_id: "sess_configured".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().join("workspace").display().to_string()),
            handle: None,
            role: None,
            agent_kind: None,
            start_command: None,
        })
        .expect("start session");

    restore_fake_tmux(original_path);
    unsafe {
        std::env::remove_var("PONTIA_HOME");
    }
    set_runtime_config(RuntimeConfig::default());

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    assert!(
        log.contains("/tmp/pontia-launch/"),
        "tmux should execute an ephemeral launch script from /tmp, got:\n{log}"
    );
    assert!(
        log.contains("; rm -f "),
        "tmux command should include outer cleanup fallback, got:\n{log}"
    );
    let launch_script = std::fs::read_to_string(launch_script_path_from_tmux_log(&log))
        .expect("ephemeral launch script");
    assert!(
        launch_script.contains("pi --approve -e /configured/clients/pi"),
        "{launch_script}"
    );
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
        std::env::set_var("PONTIA_HOME", tempdir.path().join("pontia-home"));
        std::env::set_var("PONTIA_PI_TUI_COMMAND", "pi from env");
    }
    set_runtime_config(RuntimeConfig {
        clients: HashMap::from([(
            "pi".to_string(),
            RuntimeClientConfig {
                tui_command: Some("pi from config".to_string()),
            },
        )]),
    });

    GenericRuntimeManager
        .start_session(RuntimeStartRequest {
            session_id: "sess_env_override".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().join("workspace-env").display().to_string()),
            handle: None,
            role: None,
            agent_kind: None,
            start_command: None,
        })
        .expect("start session");

    restore_fake_tmux(original_path);
    unsafe {
        std::env::remove_var("PONTIA_HOME");
        std::env::remove_var("PONTIA_PI_TUI_COMMAND");
    }
    set_runtime_config(RuntimeConfig::default());

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let launch_script = std::fs::read_to_string(launch_script_path_from_tmux_log(&log))
        .expect("ephemeral launch script");
    assert!(launch_script.contains("pi from env"), "{launch_script}");
    assert!(!launch_script.contains("pi from config"), "{launch_script}");
}

#[test]
fn terminate_tmux_pane_kills_bound_pane_when_send_keys_fails() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);

    let original_path = install_fake_tmux(tempdir.path(), &tmux_log, None);
    unsafe {
        std::env::set_var("TMUX_SEND_KEYS_FAIL", "1");
    }

    GenericRuntimeManager
        .terminate_tmux_pane("/tmp/tmux-test", "%42", &["C-c", "C-c"])
        .expect("terminate tmux pane should fallback to kill-pane");

    unsafe {
        std::env::remove_var("TMUX_SEND_KEYS_FAIL");
    }
    restore_fake_tmux(original_path);

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    assert!(
        log.lines()
            .any(|line| line == "-S /tmp/tmux-test send-keys -t %42 C-c"),
        "{log}"
    );
    assert!(
        log.lines()
            .any(|line| line == "-S /tmp/tmux-test kill-pane -t %42"),
        "{log}"
    );
    assert!(
        !log.lines().any(|line| line.contains("kill-session")),
        "{log}"
    );
}

#[test]
fn kill_tmux_pane_targets_bound_socket_and_pane() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);

    let original_path = install_fake_tmux(tempdir.path(), &tmux_log, None);

    GenericRuntimeManager
        .kill_tmux_pane("/tmp/tmux-test", "%42")
        .expect("kill tmux pane");

    restore_fake_tmux(original_path);

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    assert!(
        log.lines()
            .any(|line| line == "-S /tmp/tmux-test kill-pane -t %42"),
        "{log}"
    );
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
        .interrupt_session("/tmp/tmux-test", "%42", InterruptBehavior::TmuxInterrupt)
        .expect("interrupt session should send escape successfully");

    restore_fake_tmux(original_path);

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let escape_sends = log
        .lines()
        .filter(|line| *line == "-S /tmp/tmux-test send-keys -t %42 Escape")
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

fn launch_script_path_from_tmux_log(log: &str) -> String {
    log.split_whitespace()
        .map(|part| part.trim_matches(|ch| ch == '\'' || ch == ';'))
        .find(|part| part.starts_with("/tmp/pontia-launch/") && part.ends_with(".sh"))
        .expect("launch script path in tmux log")
        .to_string()
}

fn write_fake_tmux(path: &Path) {
    std::fs::write(
        path,
        r#"#!/usr/bin/env sh
printf '%s\n' "$*" >> "$TMUX_LOG"
if [ "$1" = "has-session" ] && [ -n "${TMUX_STATE:-}" ] && [ -f "$TMUX_STATE" ]; then
  exit 1
fi
if [ "$1" = "-S" ] && [ "$3" = "list-panes" ]; then
  printf '%%42\n'
  exit 0
fi
if [ "$1" = "-S" ] && [ "$3" = "send-keys" ] && [ -n "${TMUX_SEND_KEYS_FAIL:-}" ]; then
  exit 1
fi
if [ "$1" = "send-keys" ] && [ -n "${TMUX_STATE:-}" ]; then
  : > "$TMUX_STATE"
fi
if [ "$1" = "-S" ] && [ "$3" = "send-keys" ] && [ -n "${TMUX_STATE:-}" ]; then
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

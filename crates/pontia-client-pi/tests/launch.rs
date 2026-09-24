use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::Path,
    sync::{Mutex, OnceLock},
};

use pontia_config::{RuntimeClientConfig, RuntimeConfig};
use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest};

fn path_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn start_session_uses_configured_tui_command_when_env_is_absent() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);
    let original_path = install_fake_tmux(tempdir.path(), &tmux_log);
    unsafe {
        std::env::remove_var("PONTIA_PI_TUI_COMMAND");
    }
    let config = RuntimeConfig {
        clients: HashMap::from([(
            "pi".to_string(),
            RuntimeClientConfig {
                tui_command: Some("custom-pi --profile configured".to_string()),
            },
        )]),
    };

    let started = start_session(
        &config,
        tempdir.path().join("pontia-home").as_path(),
        RuntimeStartRequest {
            session_id: "sess_configured".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().join("workspace").display().to_string()),
            workspace_name: None,
            handle: None,
            role: None,
            start_command: None,
            environment: Default::default(),
        },
    )
    .expect("start session");

    restore_fake_tmux(original_path);
    assert_eq!(
        started.metadata["start_command"],
        "custom-pi --profile configured --approve"
    );

    let pontia_home = tempdir.path().join("pontia-home");
    let expected_launch_dir = pontia_home.join("state/launch");
    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    assert!(
        log.contains(expected_launch_dir.to_str().expect("utf-8 launch dir")),
        "tmux should execute an ephemeral launch script from PONTIA_HOME/state/launch, got:\n{log}"
    );
    assert!(
        log.contains("; rm -f "),
        "tmux command should include outer cleanup fallback, got:\n{log}"
    );
    let launch_script = std::fs::read_to_string(launch_script_path_from_tmux_log(&log))
        .expect("ephemeral launch script");
    assert!(
        launch_script.contains(&format!("export PONTIA_HOME='{}'", pontia_home.display())),
        "{launch_script}"
    );
    assert!(
        launch_script.contains("custom-pi --profile configured --approve --session-id"),
        "{launch_script}"
    );
    assert!(launch_script.contains("sess_configured"), "{launch_script}");
}

#[test]
fn start_session_prefers_env_tui_command_over_configured_command() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);
    let original_path = install_fake_tmux(tempdir.path(), &tmux_log);
    unsafe {
        std::env::set_var("PONTIA_PI_TUI_COMMAND", "custom-pi from-env");
    }
    let config = RuntimeConfig {
        clients: HashMap::from([(
            "pi".to_string(),
            RuntimeClientConfig {
                tui_command: Some("pi from config".to_string()),
            },
        )]),
    };

    start_session(
        &config,
        tempdir.path().join("pontia-home").as_path(),
        RuntimeStartRequest {
            session_id: "sess_env_override".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().join("workspace-env").display().to_string()),
            workspace_name: None,
            handle: None,
            role: None,
            start_command: None,
            environment: Default::default(),
        },
    )
    .expect("start session");

    restore_fake_tmux(original_path);
    unsafe {
        std::env::remove_var("PONTIA_PI_TUI_COMMAND");
    }

    let log = std::fs::read_to_string(tmux_log).expect("tmux log");
    let launch_script = std::fs::read_to_string(launch_script_path_from_tmux_log(&log))
        .expect("ephemeral launch script");
    assert!(
        launch_script.contains("custom-pi from-env --approve --session-id"),
        "{launch_script}"
    );
    assert!(
        launch_script.contains("sess_env_override"),
        "{launch_script}"
    );
    assert!(!launch_script.contains("pi from config"), "{launch_script}");
}

#[test]
fn kill_tmux_pane_targets_bound_socket_and_pane() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let tempdir = tempfile::tempdir().expect("tempdir");
    let tmux_log = tempdir.path().join("tmux.log");
    let fake_tmux = tempdir.path().join("tmux");
    write_fake_tmux(&fake_tmux);

    let original_path = install_fake_tmux(tempdir.path(), &tmux_log);

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
fn repeated_resume_preserves_command_and_passes_exact_quoted_session_file() {
    let _guard = path_env_lock().lock().expect("path env lock");
    let root = tempfile::tempdir().expect("test root");
    let tmux_log = root.path().join("tmux.log");
    write_fake_tmux(&root.path().join("tmux"));
    let original_path = install_fake_tmux(root.path(), &tmux_log);
    let native_dir = root.path().join("native sessions");
    std::fs::create_dir(&native_dir).unwrap();
    let native_file = native_dir.join("user's $(echo untouched) session.jsonl");
    std::fs::write(
        &native_file,
        "{\"type\":\"session\",\"id\":\"native-id\"}\n",
    )
    .unwrap();
    let capture = root.path().join("capture.sh");
    let args_file = root.path().join("args");
    std::fs::write(
        &capture,
        format!("printf '%s\\n' \"$@\" > '{}'\n", args_file.display()),
    )
    .unwrap();
    let command = format!(
        "sh '{}' --profile kept --session-dir '{}'",
        capture.display(),
        native_dir.display()
    );
    let launcher = pontia_client_pi::registration(Some("unused-pi --different-config".into()))
        .launcher
        .unwrap();
    let mut persisted = command.clone();
    for restart_count in 1..=2 {
        std::fs::write(&tmux_log, "").unwrap();
        let launched = launcher
            .launch(pontia_application::clients::ClientLaunchRequest {
                root: root.path(),
                runtime: launch_request(root.path(), Some(persisted)),
                restart_count,
                reuse_pane: None,
                native_session_key: Some("native-id"),
                native_session_file: Some(&native_file),
            })
            .expect("resume");
        let log = std::fs::read_to_string(&tmux_log).unwrap();
        let status = std::process::Command::new("sh")
            .arg(launch_script_path_from_tmux_log(&log))
            .status()
            .expect("execute launch script");
        assert!(status.success());
        let args = std::fs::read_to_string(&args_file).unwrap();
        assert_eq!(
            args.lines().collect::<Vec<_>>(),
            vec![
                "--profile",
                "kept",
                "--session-dir",
                native_dir.to_str().unwrap(),
                "--session",
                native_file.to_str().unwrap(),
            ]
        );
        persisted = launched.metadata["start_command"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(persisted, command);
    }
    restore_fake_tmux(original_path);
}

#[test]
fn resume_rejects_missing_invalid_or_different_native_session_before_launch() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("session.jsonl");
    let launcher = pontia_client_pi::registration(None).launcher.unwrap();
    assert!(launcher.validate_resume("native-id", None).is_err());
    for contents in [
        None,
        Some(""),
        Some("not-json"),
        Some("{\"type\":\"session\",\"id\":\"other-id\"}\n"),
    ] {
        if let Some(contents) = contents {
            std::fs::write(&file, contents).unwrap();
        }
        let result = launcher.launch(pontia_application::clients::ClientLaunchRequest {
            root: root.path(),
            runtime: launch_request(root.path(), Some("must-not-run".into())),
            restart_count: 1,
            reuse_pane: None,
            native_session_key: Some("native-id"),
            native_session_file: Some(&file),
        });
        assert!(
            matches!(
                result,
                Err(pontia_core::Error::CapabilityUnavailable(_))
                    | Err(pontia_core::Error::StateConflict(_))
            ),
            "{result:?}"
        );
    }
    assert!(!root.path().join("state/launch").exists());
}

fn launch_request(root: &Path, start_command: Option<String>) -> RuntimeStartRequest {
    RuntimeStartRequest {
        session_id: "sess_launch".into(),
        client_type: "pi".into(),
        workspace: Some(root.join("workspace").display().to_string()),
        workspace_name: None,
        handle: None,
        role: None,
        start_command,
        environment: Default::default(),
    }
}

fn install_fake_tmux(tempdir: &Path, tmux_log: &Path) -> String {
    let original_path = std::env::var("PATH").unwrap_or_default();
    unsafe {
        std::env::set_var("PATH", format!("{}:{original_path}", tempdir.display()));
        std::env::set_var("TMUX_LOG", tmux_log);
    }
    original_path
}

fn restore_fake_tmux(original_path: String) {
    unsafe {
        std::env::set_var("PATH", original_path);
        std::env::remove_var("TMUX_LOG");
    }
}

fn launch_script_path_from_tmux_log(log: &str) -> String {
    log.split_whitespace()
        .map(|part| part.trim_matches(|ch| ch == '\'' || ch == ';'))
        .find(|part| part.contains("/state/launch/") && part.ends_with(".sh"))
        .expect("launch script path in tmux log")
        .to_string()
}

fn write_fake_tmux(path: &Path) {
    std::fs::write(
        path,
        r#"#!/usr/bin/env sh
printf '%s\n' "$*" >> "$TMUX_LOG"
if [ "$1" = "-S" ] && [ "$3" = "list-panes" ]; then
  printf '%%42\n'
  exit 0
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

fn start_session(
    config: &RuntimeConfig,
    root: &Path,
    runtime: RuntimeStartRequest,
) -> pontia_core::Result<pontia_runtime::RuntimeStartResult> {
    pontia_client_pi::registration(config.tui_command_for_client_config_key("pi"))
        .launcher
        .unwrap()
        .launch(pontia_application::clients::ClientLaunchRequest {
            root,
            runtime,
            restart_count: 0,
            reuse_pane: None,
            native_session_key: None,
            native_session_file: None,
        })
}

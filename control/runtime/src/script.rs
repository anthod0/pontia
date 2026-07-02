use std::{
    fs::OpenOptions,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use pontia_agent_clients::{
    self as agent_clients, DispatchBehavior, RuntimeBehavior, TmuxRuntimeBehavior,
};
use pontia_core::error::{Error, Result};

use super::{
    RuntimeStartRequest,
    config::{configured_internal_event_url, configured_tui_command},
    utils::shell_quote,
};

pub(super) struct RuntimePaths<'a> {
    pub(super) log_path: &'a Path,
}

pub(crate) fn tmux_start_command(
    request: &RuntimeStartRequest,
    tmux_runtime: TmuxRuntimeBehavior,
    quote_session_id: bool,
) -> String {
    let Some(command) = request.start_command.clone() else {
        let mut command = tmux_runtime
            .command_env
            .and_then(|env| std::env::var(env).ok())
            .or_else(|| configured_tui_command(&request.client_type))
            .unwrap_or_else(|| tmux_runtime.default_command.to_string());
        for arg in tmux_runtime.startup_args {
            command.push(' ');
            command.push_str(arg);
        }
        if let Some(session_identity_arg) = tmux_runtime.session_identity_arg {
            command.push(' ');
            command.push_str(session_identity_arg);
            command.push(' ');
            if quote_session_id {
                command.push_str(&shell_quote(&request.session_id));
            } else {
                command.push_str(&request.session_id);
            }
        }
        return command;
    };
    command
}

pub(super) fn write_ephemeral_launch_script(
    workspace: &Path,
    runtime_paths: &RuntimePaths<'_>,
    request: &RuntimeStartRequest,
    runtime_instance_id: &str,
) -> Result<PathBuf> {
    let launch_dir = pontia_home_path_for_export().join("state/launch");
    std::fs::create_dir_all(&launch_dir)?;
    let path = launch_dir.join(format!("{runtime_instance_id}.sh"));
    write_launch_script(
        &path,
        workspace,
        runtime_paths,
        request,
        runtime_instance_id,
    )?;
    let mut permissions = std::fs::metadata(&path)?.permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&path, permissions)?;
    Ok(path)
}

pub(super) fn write_launch_script(
    path: &Path,
    workspace: &Path,
    runtime_paths: &RuntimePaths<'_>,
    request: &RuntimeStartRequest,
    runtime_instance_id: &str,
) -> Result<()> {
    let client_spec = agent_clients::get_client_spec(&request.client_type).ok_or_else(|| {
        Error::Domain(format!("unsupported client_type: {}", request.client_type))
    })?;
    let (log_setup, runtime_body) = match client_spec.adapter.runtime {
        RuntimeBehavior::Tmux(tmux_runtime) => {
            let command = tmux_start_command(request, tmux_runtime, true);
            (
                format!(
                    "echo {} >> \"$PONTIA_RUNTIME_LOG\"",
                    shell_quote(&format!(
                        "session={} runtime_instance={} pontia runtime started",
                        request.session_id, runtime_instance_id
                    ))
                ),
                format!("exec sh -lc {}\n", shell_quote(&command)),
            )
        }
        RuntimeBehavior::InProcess => match client_spec.adapter.dispatch {
            DispatchBehavior::InProcessRecorded | DispatchBehavior::None => (
                format!(
                    "exec >> \"$PONTIA_RUNTIME_LOG\" 2>&1\necho {}",
                    shell_quote(&format!(
                        "session={} runtime_instance={} pontia runtime started",
                        request.session_id, runtime_instance_id
                    ))
                ),
                "trap 'exit 0' TERM INT\nwhile :; do sleep 60; done\n".to_string(),
            ),
            DispatchBehavior::TmuxPaste => {
                return Err(Error::Domain(format!(
                    "{} cannot use tmux paste dispatch without tmux runtime",
                    request.client_type
                )));
            }
        },
    };
    let agent_kind_export = request
        .agent_kind
        .as_ref()
        .map(|agent_kind| format!("export PONTIA_AGENT_KIND={}\n", shell_quote(agent_kind)))
        .unwrap_or_default();
    let content = format!(
        r#"#!/usr/bin/env sh
export PONTIA_SESSION_ID={}
export PONTIA_CLIENT_TYPE={}
export PONTIA_WORKSPACE={}
export PONTIA_HOME={}
export PONTIA_RUNTIME_LOG={}
export PONTIA_RUNTIME_INSTANCE_ID={}
PONTIA_LAUNCH_SCRIPT=${{0:-}}
cleanup_pontia_launch_script() {{
  if [ -n "$PONTIA_LAUNCH_SCRIPT" ]; then
    rm -f "$PONTIA_LAUNCH_SCRIPT"
  fi
}}
trap cleanup_pontia_launch_script EXIT HUP INT TERM
{}{}
cleanup_pontia_launch_script
{}
"#,
        shell_quote(&request.session_id),
        shell_quote(&request.client_type),
        shell_quote(&workspace.display().to_string()),
        shell_quote(&pontia_home_for_export()),
        shell_quote(&runtime_paths.log_path.display().to_string()),
        shell_quote(runtime_instance_id),
        agent_kind_export,
        log_setup,
        runtime_body,
    );
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o700)
        .open(path)?;
    file.write_all(content.as_bytes())?;
    Ok(())
}

pub(super) fn shell_quote_path(path: &Path) -> String {
    shell_quote(&path.display().to_string())
}

fn pontia_home_for_export() -> String {
    pontia_home_path_for_export().display().to_string()
}

fn pontia_home_path_for_export() -> PathBuf {
    std::env::var("PONTIA_HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(".pontia"))
                .unwrap_or_else(|_| PathBuf::from(".pontia"))
        })
}

pub(super) fn internal_event_url() -> String {
    configured_internal_event_url().unwrap_or_else(|| default_internal_event_url().to_string())
}

#[cfg(test)]
fn default_internal_event_url() -> &'static str {
    "http://127.0.0.1:9/internal/v1/events"
}

#[cfg(not(test))]
fn default_internal_event_url() -> &'static str {
    "http://127.0.0.1:8080/internal/v1/events"
}

pub(super) fn run_startup_hooks(
    hooks: &[agent_clients::StartupHook],
    workspace: &Path,
) -> Result<()> {
    agent_clients::run_startup_hooks(hooks, workspace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_runtime_script_uses_exact_project_session_id() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let script_path = tempdir.path().join("launch.sh");
        let log_path = tempdir.path().join("runtime.log");
        let paths = RuntimePaths {
            log_path: &log_path,
        };
        let request = RuntimeStartRequest {
            session_id: "sess_resume_1".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().display().to_string()),
            workspace_name: None,
            handle: None,
            role: None,
            agent_kind: None,
            start_command: None,
        };

        write_launch_script(
            &script_path,
            tempdir.path(),
            &paths,
            &request,
            "runtime_instance_1",
        )
        .expect("write script");

        let script = std::fs::read_to_string(script_path).expect("script");
        assert!(
            script.contains("pi --approve --session-id"),
            "script was:\n{script}"
        );
        assert!(script.contains("sess_resume_1"), "script was:\n{script}");
        assert!(
            script.contains("session=sess_resume_1 runtime_instance=runtime_instance_1"),
            "script was:\n{script}"
        );
    }

    #[test]
    fn runtime_script_prefers_explicit_start_command() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let paths = RuntimePaths {
            log_path: &tempdir.path().join("runtime.log"),
        };
        let request = RuntimeStartRequest {
            session_id: "sess_explicit_start".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().display().to_string()),
            workspace_name: None,
            handle: None,
            role: None,
            agent_kind: None,
            start_command: Some("pi --resume-user-command".to_string()),
        };
        let script_path = tempdir.path().join("launch.sh");

        write_launch_script(
            &script_path,
            tempdir.path(),
            &paths,
            &request,
            "rtinst_explicit",
        )
        .expect("write launch script");

        let content = std::fs::read_to_string(script_path).expect("launch script");
        assert!(content.contains("pi --resume-user-command"));
        assert!(!content.contains("--session-id sess_explicit_start"));
    }

    #[test]
    fn runtime_script_internal_event_url_uses_test_default_and_configured_bind_port() {
        crate::reset_runtime_bind_addr_for_tests();
        assert_eq!(
            default_internal_event_url(),
            "http://127.0.0.1:9/internal/v1/events"
        );

        crate::set_runtime_bind_addr("127.0.0.1:18080".parse().expect("bind addr"));
        assert_eq!(
            internal_event_url(),
            "http://127.0.0.1:18080/internal/v1/events"
        );

        crate::set_runtime_bind_addr("0.0.0.0:18081".parse().expect("bind addr"));
        assert_eq!(
            internal_event_url(),
            "http://127.0.0.1:18081/internal/v1/events"
        );
    }
}

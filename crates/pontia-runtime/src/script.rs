use std::{
    fs::OpenOptions,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use pontia_agent_clients::{self as agent_clients, DispatchBehavior, RuntimeBehavior};
use pontia_core::error::{Error, Result};

use super::RuntimeStartRequest;

pub(super) struct RuntimePaths<'a> {
    pub(super) log_path: &'a Path,
}

pub(super) fn write_ephemeral_launch_script(
    pontia_home: &Path,
    runtime_paths: &RuntimePaths<'_>,
    request: &RuntimeStartRequest,
    launch_id: &str,
    runtime_instance_id: &str,
    client_spec: &agent_clients::AgentClientSpec,
) -> Result<PathBuf> {
    let launch_dir = pontia_home.join("state/launch");
    std::fs::create_dir_all(&launch_dir)?;
    let path = launch_dir.join(format!("{launch_id}.sh"));
    write_launch_script(
        &path,
        pontia_home,
        runtime_paths,
        request,
        launch_id,
        runtime_instance_id,
        client_spec,
    )?;
    let mut permissions = std::fs::metadata(&path)?.permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&path, permissions)?;
    Ok(path)
}

pub(super) fn write_launch_script(
    path: &Path,
    pontia_home: &Path,
    runtime_paths: &RuntimePaths<'_>,
    request: &RuntimeStartRequest,
    launch_id: &str,
    runtime_instance_id: &str,
    client_spec: &agent_clients::AgentClientSpec,
) -> Result<()> {
    let runtime_environment = render_runtime_environment(request)?;
    let (log_setup, runtime_body) = match client_spec.adapter.runtime {
        RuntimeBehavior::CodexAppServer => {
            return Err(Error::Domain("Codex uses a managed app-server".into()));
        }
        RuntimeBehavior::Tmux(_) => {
            let command = request
                .start_command
                .as_deref()
                .ok_or_else(|| Error::Domain("tmux launch requires a command".into()))?;
            (
                format!(
                    "echo {} >> {}",
                    shell_quote(&format!(
                        "session={} launch={} pontia runtime started",
                        request.session_id, launch_id
                    )),
                    shell_quote_path(runtime_paths.log_path),
                ),
                format!("exec sh -lc {}\n", shell_quote(command)),
            )
        }
        RuntimeBehavior::InProcess => match client_spec.adapter.dispatch {
            DispatchBehavior::InProcessRecorded | DispatchBehavior::None => (
                format!(
                    "exec >> {} 2>&1\necho {}",
                    shell_quote_path(runtime_paths.log_path),
                    shell_quote(&format!(
                        "session={} launch={} pontia runtime started",
                        request.session_id, launch_id
                    )),
                ),
                "trap 'exit 0' TERM INT\nwhile :; do sleep 60; done\n".to_string(),
            ),
            DispatchBehavior::Connected | DispatchBehavior::CodexProtocol => {
                return Err(Error::Domain(format!(
                    "{} cannot use client protocol dispatch with an in-process runtime",
                    request.client_type
                )));
            }
        },
    };
    let content = format!(
        r#"#!/usr/bin/env sh
unset PONTIA_SESSION_ID PONTIA_CLIENT_TYPE PONTIA_RUNTIME_INSTANCE_ID PONTIA_WORKSPACE PONTIA_RUNTIME_LOG PONTIA_WORKFLOW_ID PONTIA_WORKFLOW_PATCH_ID
export PONTIA_HOME={}
{}if [ -n "${{TMUX:-}}" ] && [ -n "${{TMUX_PANE:-}}" ]; then
  tmux set-option -p -t "$TMUX_PANE" @pontia_session_id {} || exit 1
  tmux set-option -p -t "$TMUX_PANE" @pontia_runtime_instance_id {} || exit 1
fi
PONTIA_LAUNCH_SCRIPT=${{0:-}}
cleanup_pontia_launch_script() {{
  if [ -n "$PONTIA_LAUNCH_SCRIPT" ]; then
    rm -f "$PONTIA_LAUNCH_SCRIPT"
  fi
}}
trap cleanup_pontia_launch_script EXIT HUP INT TERM
{}
cleanup_pontia_launch_script
{}
"#,
        shell_quote(&pontia_home.display().to_string()),
        runtime_environment,
        shell_quote(&request.session_id),
        shell_quote(runtime_instance_id),
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

fn render_runtime_environment(request: &RuntimeStartRequest) -> Result<String> {
    let mut rendered = String::new();
    for (name, value) in &request.environment {
        let valid_name = name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        });
        if !valid_name || name == "PONTIA_HOME" {
            return Err(Error::Domain(format!(
                "invalid runtime environment variable name: {name}"
            )));
        }
        rendered.push_str(&format!("export {name}={}\n", shell_quote(value)));
    }
    Ok(rendered)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn shell_quote_path(path: &Path) -> String {
    shell_quote(&path.display().to_string())
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
    fn runtime_script_uses_exact_project_session_id() {
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
            start_command: Some("test-agent --approve --session-id sess_resume_1".into()),
            environment: [
                ("PONTIA_WORKFLOW_ID".to_string(), "wf_123".to_string()),
                (
                    "PONTIA_WORKFLOW_PATCH_ID".to_string(),
                    "patch_123".to_string(),
                ),
            ]
            .into_iter()
            .collect(),
        };

        write_launch_script(
            &script_path,
            tempdir.path(),
            &paths,
            &request,
            "launch_1",
            "runtime_instance_1",
            &crate::test_tmux_spec(),
        )
        .expect("write script");

        let script = std::fs::read_to_string(script_path).expect("script");
        assert!(
            script.contains("test-agent --approve --session-id"),
            "script was:\n{script}"
        );
        assert!(script.contains("sess_resume_1"), "script was:\n{script}");
        assert!(script.contains("export PONTIA_HOME="));
        assert!(script.contains("export PONTIA_WORKFLOW_ID='wf_123'"));
        assert!(script.contains("export PONTIA_WORKFLOW_PATCH_ID='patch_123'"));
        assert!(script.contains(
            "unset PONTIA_SESSION_ID PONTIA_CLIENT_TYPE PONTIA_RUNTIME_INSTANCE_ID PONTIA_WORKSPACE PONTIA_RUNTIME_LOG PONTIA_WORKFLOW_ID PONTIA_WORKFLOW_PATCH_ID"
        ));
        for name in [
            "PONTIA_SESSION_ID",
            "PONTIA_CLIENT_TYPE",
            "PONTIA_RUNTIME_INSTANCE_ID",
            "PONTIA_WORKSPACE",
            "PONTIA_RUNTIME_LOG",
        ] {
            assert!(
                !script.contains(&format!("export {name}=")),
                "script unexpectedly exported {name}:\n{script}"
            );
        }
        assert!(
            script.contains(
                "tmux set-option -p -t \"$TMUX_PANE\" @pontia_session_id 'sess_resume_1'"
            )
        );
        assert!(script.contains(
            "tmux set-option -p -t \"$TMUX_PANE\" @pontia_runtime_instance_id 'runtime_instance_1'"
        ));
        assert!(
            script.contains("session=sess_resume_1 launch=launch_1"),
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
            start_command: Some("pi --resume-user-command".to_string()),
            environment: Default::default(),
        };
        let script_path = tempdir.path().join("launch.sh");

        write_launch_script(
            &script_path,
            tempdir.path(),
            &paths,
            &request,
            "launch_explicit",
            "rtinst_explicit",
            &crate::test_tmux_spec(),
        )
        .expect("write launch script");

        let content = std::fs::read_to_string(script_path).expect("launch script");
        assert!(content.contains("pi --resume-user-command"));
        assert!(!content.contains("--session-id sess_explicit_start"));
    }
}

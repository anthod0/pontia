use std::path::Path;

use crate::{
    agent_clients::{self, DispatchBehavior, HookLogBehavior, RuntimeBehavior, StartupHook},
    error::{Error, Result},
};

use super::{
    RuntimeStartRequest, claude_code,
    config::{configured_external_api_token, configured_tui_command},
    shell_quote,
};

pub(super) struct RuntimePaths<'a> {
    pub(super) runtime_dir: &'a Path,
    pub(super) log_path: &'a Path,
    pub(super) adapter_event_log: &'a Path,
    pub(super) current_turn_file: &'a Path,
}

pub(super) fn write_runtime_script(
    path: &Path,
    workspace: &Path,
    runtime_paths: &RuntimePaths<'_>,
    request: &RuntimeStartRequest,
    runtime_instance_id: &str,
) -> Result<()> {
    let client_spec = agent_clients::get_client_spec(&request.client_type).ok_or_else(|| {
        Error::Domain(format!("unsupported client_type: {}", request.client_type))
    })?;
    let (log_setup, runtime_body) = match client_spec.runtime {
        RuntimeBehavior::Tmux(tmux_runtime) => {
            let mut command = tmux_runtime
                .command_env
                .and_then(|env| std::env::var(env).ok())
                .or_else(|| configured_tui_command(&request.client_type))
                .unwrap_or_else(|| tmux_runtime.default_command.to_string());
            if let Some(session_identity_arg) = tmux_runtime.session_identity_arg
                && command.trim() == tmux_runtime.default_command
            {
                command.push(' ');
                command.push_str(session_identity_arg);
                command.push(' ');
                command.push_str(&shell_quote(&request.session_id));
            }
            (
                "echo \"llmparty runtime started\" >> \"$LLMPARTY_RUNTIME_LOG\"".to_string(),
                format!("exec sh -lc {}\n", shell_quote(&command)),
            )
        }
        RuntimeBehavior::InProcessTest => match client_spec.dispatch {
            DispatchBehavior::GenericTestAdapter | DispatchBehavior::None => (
                "exec >> \"$LLMPARTY_RUNTIME_LOG\" 2>&1\necho \"llmparty runtime started\""
                    .to_string(),
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
        .map(|agent_kind| format!("export LLMPARTY_AGENT_KIND={}\n", shell_quote(agent_kind)))
        .unwrap_or_default();
    let hook_log_export = client_spec
        .tmux_runtime()
        .and_then(|runtime| runtime.hook_log)
        .map(|hook_log| {
            let path = hook_log_path(runtime_paths, hook_log);
            format!(
                "export {}={}\n",
                hook_log.env,
                shell_quote(&path.display().to_string())
            )
        })
        .unwrap_or_default();
    let content = format!(
        r#"#!/usr/bin/env sh
export LLMPARTY_SESSION_ID={}
export LLMPARTY_CLIENT_TYPE={}
export LLMPARTY_WORKSPACE={}
export LLMPARTY_RUNTIME_DIR={}
export LLMPARTY_RUNTIME_LOG={}
export LLMPARTY_ADAPTER_EVENT_LOG={}
export LLMPARTY_CURRENT_TURN_FILE={}
export LLMPARTY_INTERNAL_EVENT_URL={}
export LLMPARTY_EXTERNAL_API_URL={}
export LLMPARTY_EXTERNAL_API_TOKEN={}
export LLMPARTY_RUNTIME_INSTANCE_ID={}
{}{}{}
{}
"#,
        shell_quote(&request.session_id),
        shell_quote(&request.client_type),
        shell_quote(&workspace.display().to_string()),
        shell_quote(&runtime_paths.runtime_dir.display().to_string()),
        shell_quote(&runtime_paths.log_path.display().to_string()),
        shell_quote(&runtime_paths.adapter_event_log.display().to_string()),
        shell_quote(&runtime_paths.current_turn_file.display().to_string()),
        shell_quote(&internal_event_url()),
        shell_quote(&external_api_url()),
        shell_quote(&external_api_token()),
        shell_quote(runtime_instance_id),
        hook_log_export,
        agent_kind_export,
        log_setup,
        runtime_body,
    );
    std::fs::write(path, content)?;
    Ok(())
}

fn hook_log_path(
    runtime_paths: &RuntimePaths<'_>,
    hook_log: HookLogBehavior,
) -> std::path::PathBuf {
    runtime_paths.runtime_dir.join(hook_log.file_name)
}

pub(super) fn internal_event_url() -> String {
    std::env::var("LLMPARTY_INTERNAL_EVENT_URL")
        .unwrap_or_else(|_| default_internal_event_url().to_string())
}

fn external_api_url() -> String {
    std::env::var("LLMPARTY_EXTERNAL_API_URL")
        .unwrap_or_else(|_| default_external_api_url().to_string())
}

#[cfg(test)]
fn default_internal_event_url() -> &'static str {
    "http://127.0.0.1:9/internal/v1/events"
}

#[cfg(not(test))]
fn default_internal_event_url() -> &'static str {
    "http://127.0.0.1:8080/internal/v1/events"
}

#[cfg(test)]
fn default_external_api_url() -> &'static str {
    "http://127.0.0.1:9/external/v1"
}

#[cfg(not(test))]
fn default_external_api_url() -> &'static str {
    "http://127.0.0.1:8080/external/v1"
}

fn external_api_token() -> String {
    configured_external_api_token()
        .or_else(|| std::env::var("LLMPARTY_EXTERNAL_API_TOKEN").ok())
        .unwrap_or_default()
}

pub(super) fn run_startup_hooks(hooks: &[StartupHook], workspace: &Path) -> Result<()> {
    for hook in hooks {
        match hook {
            StartupHook::ClaudeCodeTrustWorkspace => {
                claude_code::ensure_workspace_trusted(workspace)?
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pi_runtime_script_uses_exact_project_session_id() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let script_path = tempdir.path().join("runtime.sh");
        let log_path = tempdir.path().join("runtime.log");
        let paths = RuntimePaths {
            runtime_dir: tempdir.path(),
            log_path: &log_path,
            adapter_event_log: &tempdir.path().join("adapter-events.jsonl"),
            current_turn_file: &tempdir.path().join("current-turn.json"),
        };
        let request = RuntimeStartRequest {
            session_id: "sess_resume_1".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().display().to_string()),
            handle: None,
            role: None,
            agent_kind: None,
        };

        write_runtime_script(
            &script_path,
            tempdir.path(),
            &paths,
            &request,
            "runtime_instance_1",
        )
        .expect("write script");

        let script = std::fs::read_to_string(script_path).expect("script");
        assert!(script.contains("pi --session-id"), "script was:\n{script}");
        assert!(script.contains("sess_resume_1"), "script was:\n{script}");
    }

    #[tokio::test]
    async fn runtime_script_uses_configured_external_api_token_when_env_is_unset() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let script_path = tempdir.path().join("runtime.sh");
        let log_path = tempdir.path().join("runtime.log");
        let paths = RuntimePaths {
            runtime_dir: tempdir.path(),
            log_path: &log_path,
            adapter_event_log: &tempdir.path().join("adapter-events.jsonl"),
            current_turn_file: &tempdir.path().join("current-turn.json"),
        };
        let request = RuntimeStartRequest {
            session_id: "sess_token_from_config".to_string(),
            client_type: "pi".to_string(),
            workspace: Some(tempdir.path().display().to_string()),
            handle: None,
            role: None,
            agent_kind: Some("planner".to_string()),
        };

        unsafe {
            std::env::remove_var("LLMPARTY_EXTERNAL_API_TOKEN");
        }
        crate::runtime::set_runtime_external_api_token(None);
        let config = crate::config::AppConfig {
            bind_addr: "127.0.0.1:0".parse().expect("bind addr"),
            database_url: format!("sqlite://{}", tempdir.path().join("llmparty.db").display()),
            external_api_token: Some("config-token".to_string()),
            run_migrations: false,
            default_client_type: "pi".to_string(),
            graph: Default::default(),
            workspace_browser: Default::default(),
            runtime: Default::default(),
            dashboard: crate::config::DashboardConfig::default(),
        };
        let _state = crate::application::initialize(&config)
            .await
            .expect("initialize app state");
        write_runtime_script(
            &script_path,
            tempdir.path(),
            &paths,
            &request,
            "runtime_instance_1",
        )
        .expect("write script");
        crate::runtime::set_runtime_external_api_token(None);

        let script = std::fs::read_to_string(script_path).expect("script");
        assert!(
            script.contains("export LLMPARTY_EXTERNAL_API_TOKEN='config-token'"),
            "script was:\n{script}"
        );
        assert!(script.contains("export LLMPARTY_AGENT_KIND='planner'"));
    }

    #[test]
    fn runtime_script_exports_only_current_client_hook_log_env() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let script_path = tempdir.path().join("runtime.sh");
        let log_path = tempdir.path().join("runtime.log");
        let paths = RuntimePaths {
            runtime_dir: tempdir.path(),
            log_path: &log_path,
            adapter_event_log: &tempdir.path().join("adapter-events.jsonl"),
            current_turn_file: &tempdir.path().join("current-turn.json"),
        };
        let request = RuntimeStartRequest {
            session_id: "sess_claude_1".to_string(),
            client_type: "claude_code".to_string(),
            workspace: Some(tempdir.path().display().to_string()),
            handle: None,
            role: None,
            agent_kind: None,
        };

        write_runtime_script(
            &script_path,
            tempdir.path(),
            &paths,
            &request,
            "runtime_instance_1",
        )
        .expect("write script");

        let script = std::fs::read_to_string(script_path).expect("script");
        assert!(
            script.contains("export LLMPARTY_CLAUDE_HOOK_LOG="),
            "script was:\n{script}"
        );
        assert!(
            !script.contains("export LLMPARTY_PI_HOOK_LOG="),
            "script was:\n{script}"
        );
    }

    #[test]
    fn test_defaults_use_non_listening_ports() {
        assert_eq!(
            default_internal_event_url(),
            "http://127.0.0.1:9/internal/v1/events"
        );
        assert_eq!(default_external_api_url(), "http://127.0.0.1:9/external/v1");
    }
}

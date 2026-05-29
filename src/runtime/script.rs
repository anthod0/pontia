use std::path::Path;

use crate::{
    agent_clients::{self, DispatchMode, StartupHook},
    error::{Error, Result},
};

use super::{RuntimeStartRequest, claude_code, config::configured_tui_command, shell_quote};

pub(super) struct RuntimePaths<'a> {
    pub(super) runtime_dir: &'a Path,
    pub(super) log_path: &'a Path,
    pub(super) adapter_event_log: &'a Path,
    pub(super) current_turn_file: &'a Path,
    pub(super) pi_hook_log: &'a Path,
    pub(super) claude_hook_log: &'a Path,
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
    let (log_setup, runtime_body) = match client_spec.dispatch_mode {
        DispatchMode::TmuxPaste => {
            let default_command = client_spec.default_command.ok_or_else(|| {
                Error::Domain(format!(
                    "{} tmux runtime missing default command",
                    request.client_type
                ))
            })?;
            let mut command = client_spec
                .command_env
                .and_then(|env| std::env::var(env).ok())
                .or_else(|| configured_tui_command(&request.client_type))
                .unwrap_or_else(|| default_command.to_string());
            if request.client_type == "pi" && command.trim() == "pi" {
                command.push_str(" --session-id ");
                command.push_str(&shell_quote(&request.session_id));
            }
            (
                "echo \"llmparty runtime started\" >> \"$LLMPARTY_RUNTIME_LOG\"".to_string(),
                format!("exec sh -lc {}\n", shell_quote(&command)),
            )
        }
        DispatchMode::GenericTestAdapter | DispatchMode::None => (
            "exec >> \"$LLMPARTY_RUNTIME_LOG\" 2>&1\necho \"llmparty runtime started\"".to_string(),
            "trap 'exit 0' TERM INT\nwhile :; do sleep 60; done\n".to_string(),
        ),
    };
    let agent_kind_export = request
        .agent_kind
        .as_ref()
        .map(|agent_kind| format!("export LLMPARTY_AGENT_KIND={}\n", shell_quote(agent_kind)))
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
export LLMPARTY_PI_HOOK_LOG={}
export LLMPARTY_CLAUDE_HOOK_LOG={}
{}{}
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
        shell_quote(&runtime_paths.pi_hook_log.display().to_string()),
        shell_quote(&runtime_paths.claude_hook_log.display().to_string()),
        agent_kind_export,
        log_setup,
        runtime_body,
    );
    std::fs::write(path, content)?;
    Ok(())
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
    std::env::var("LLMPARTY_EXTERNAL_API_TOKEN").unwrap_or_default()
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
            pi_hook_log: &tempdir.path().join("pi-hook.log"),
            claude_hook_log: &tempdir.path().join("claude-hook.log"),
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

    #[test]
    fn test_defaults_use_non_listening_ports() {
        assert_eq!(
            default_internal_event_url(),
            "http://127.0.0.1:9/internal/v1/events"
        );
        assert_eq!(default_external_api_url(), "http://127.0.0.1:9/external/v1");
    }
}

//! Runtime control boundary.
//!
//! The MVP generic runtime records a binding and immediately reports ready. This
//! module stays independent from HTTP transport details.

use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use crate::{
    adapters::{AdapterCapabilities, AgentEventSource, AgentInputSink, GenericTestAdapter},
    application::SessionCapabilities,
    error::{Error, Result},
    time::utc_now,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStartRequest {
    pub session_id: String,
    pub client_type: String,
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeStartResult {
    pub runtime_kind: String,
    pub runtime_ref: String,
    pub capabilities: SessionCapabilities,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentInput {
    pub session_id: String,
    pub turn_id: String,
    pub input: String,
}

#[derive(Debug, Clone, Default)]
pub struct GenericRuntimeManager;

impl From<AdapterCapabilities> for SessionCapabilities {
    fn from(capabilities: AdapterCapabilities) -> Self {
        Self {
            accept_task: capabilities.accept_task,
            report_turn_started: capabilities.report_turn_started,
            report_turn_finished: capabilities.report_turn_finished,
            interrupt: capabilities.interrupt,
            stream_output: capabilities.stream_output,
            heartbeat: capabilities.heartbeat,
            artifact_sources: capabilities.artifact_sources,
        }
    }
}

impl GenericRuntimeManager {
    pub fn start_session(&self, request: RuntimeStartRequest) -> Result<RuntimeStartResult> {
        self.start_session_with_restart_count(request, 0)
    }

    pub fn start_session_with_restart_count(
        &self,
        request: RuntimeStartRequest,
        restart_count: i64,
    ) -> Result<RuntimeStartResult> {
        let capabilities = capabilities_for_client(&request.client_type);
        let tmux_session = tmux_session_name(&request.session_id);
        let workspace = workspace_path(&request)?;
        let runtime_dir = runtime_dir(&request.session_id)?;
        std::fs::create_dir_all(&runtime_dir)?;
        let log_path = runtime_dir.join("runtime.log");
        let adapter_event_log = runtime_dir.join("adapter-events.jsonl");
        let current_turn_file = runtime_dir.join("current-turn.json");
        let pi_hook_log = runtime_dir.join("pi-hook.log");
        let claude_hook_log = runtime_dir.join("claude-hook.log");
        let internal_event_url = internal_event_url();
        std::fs::File::create(&log_path)?;
        let script_path = runtime_dir.join("runtime.sh");
        let runtime_paths = RuntimePaths {
            runtime_dir: &runtime_dir,
            log_path: &log_path,
            adapter_event_log: &adapter_event_log,
            current_turn_file: &current_turn_file,
            pi_hook_log: &pi_hook_log,
            claude_hook_log: &claude_hook_log,
        };
        write_runtime_script(&script_path, &workspace, &runtime_paths, &request)?;

        let status = spawn_tmux_session(&tmux_session, &workspace, &script_path)
            .map_err(|err| Error::Domain(format!("tmux runtime spawn failed: {err}")))?;
        if !status.success() {
            return Err(Error::Domain(format!(
                "tmux runtime spawn failed with status {status}"
            )));
        }

        let started_at = utc_now()
            .format(&Rfc3339)
            .map_err(|err| Error::Domain(format!("invalid runtime timestamp: {err}")))?;
        let workspace = workspace.display().to_string();
        let runtime_dir = runtime_dir.display().to_string();
        let log_path = log_path.display().to_string();
        let adapter_event_log = adapter_event_log.display().to_string();
        let current_turn_file = current_turn_file.display().to_string();
        let pi_hook_log = pi_hook_log.display().to_string();
        let claude_hook_log = claude_hook_log.display().to_string();
        Ok(RuntimeStartResult {
            runtime_kind: "tmux".to_string(),
            runtime_ref: tmux_session.clone(),
            capabilities: capabilities.into(),
            metadata: json!({
                "backend": "tmux",
                "tmux_session": tmux_session,
                "workspace": workspace,
                "runtime_dir": runtime_dir,
                "runtime_log": log_path,
                "log_path": log_path,
                "adapter_event_log": adapter_event_log,
                "current_turn_file": current_turn_file,
                "internal_event_url": internal_event_url,
                "pi_hook_log": pi_hook_log,
                "claude_hook_log": claude_hook_log,
                "started_at": started_at,
                "restart_count": restart_count,
            }),
        })
    }

    pub fn submit_input(&self, input: AgentInput) -> Result<()> {
        GenericTestAdapter.accept_input(input)
    }

    pub fn dispatch_pi_turn(&self, runtime_ref: &str, input: &AgentInput) -> Result<()> {
        self.dispatch_tui_turn(runtime_ref, "pi", input)
    }

    pub fn dispatch_tui_turn(
        &self,
        runtime_ref: &str,
        client_type: &str,
        input: &AgentInput,
    ) -> Result<()> {
        if !self.is_alive(runtime_ref) {
            return Err(Error::Domain(format!(
                "{client_type} runtime {runtime_ref} is not alive"
            )));
        }

        let buffer_name = format!("llmparty_{}", sanitize_tmux_identifier(&input.turn_id));
        let mut child = Command::new("tmux")
            .args(["load-buffer", "-b", &buffer_name, "-"])
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|err| Error::Domain(format!("tmux dispatch buffer failed: {err}")))?;
        {
            let stdin = child.stdin.as_mut().ok_or_else(|| {
                Error::Domain("tmux dispatch buffer stdin unavailable".to_string())
            })?;
            stdin.write_all(input.input.as_bytes())?;
        }
        let status = child
            .wait()
            .map_err(|err| Error::Domain(format!("tmux dispatch buffer failed: {err}")))?;
        if !status.success() {
            return Err(Error::Domain(format!(
                "tmux dispatch buffer failed with status {status}"
            )));
        }

        let status = Command::new("tmux")
            .args(["paste-buffer", "-t", runtime_ref, "-b", &buffer_name])
            .status()
            .map_err(|err| Error::Domain(format!("tmux dispatch paste failed: {err}")))?;
        if !status.success() {
            return Err(Error::Domain(format!(
                "tmux dispatch paste failed with status {status}"
            )));
        }

        let status = Command::new("tmux")
            .args(["send-keys", "-t", runtime_ref, "Enter"])
            .status()
            .map_err(|err| Error::Domain(format!("tmux dispatch submit failed: {err}")))?;
        if !status.success() {
            return Err(Error::Domain(format!(
                "tmux dispatch submit failed with status {status}"
            )));
        }

        let _ = Command::new("tmux")
            .args(["delete-buffer", "-b", &buffer_name])
            .status();
        Ok(())
    }

    pub fn terminate_session(&self, runtime_ref: &str) -> Result<()> {
        let status = Command::new("tmux")
            .args(["kill-session", "-t", runtime_ref])
            .stderr(Stdio::null())
            .status()
            .map_err(|err| Error::Domain(format!("tmux runtime terminate failed: {err}")))?;
        if status.success() || !self.is_alive(runtime_ref) {
            Ok(())
        } else {
            Err(Error::Domain(format!(
                "tmux runtime terminate failed with status {status}"
            )))
        }
    }

    pub fn restart_session(&self, request: RuntimeStartRequest) -> Result<RuntimeStartResult> {
        self.start_session(request)
    }

    pub fn is_alive(&self, runtime_ref: &str) -> bool {
        Command::new("tmux")
            .args(["has-session", "-t", runtime_ref])
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
}

impl RuntimeStartResult {
    pub fn binding_metadata(&self) -> serde_json::Value {
        let mut metadata = self.metadata.clone();
        if let Some(object) = metadata.as_object_mut() {
            object.insert("capabilities".to_string(), json!(self.capabilities));
        }
        metadata
    }
}

fn spawn_tmux_session(
    tmux_session: &str,
    workspace: &Path,
    script_path: &Path,
) -> std::io::Result<ExitStatus> {
    let command = || {
        Command::new("tmux")
            .args([
                "new-session",
                "-d",
                "-s",
                tmux_session,
                "-c",
                &workspace.display().to_string(),
                &format!("sh {}", shell_quote(&script_path.display().to_string())),
            ])
            .status()
    };

    let first = command()?;
    if first.success() {
        return Ok(first);
    }

    thread::sleep(Duration::from_millis(50));
    command()
}

fn capabilities_for_client(client_type: &str) -> AdapterCapabilities {
    match client_type {
        "generic" => GenericTestAdapter.capabilities(),
        "pi" => AdapterCapabilities::pi_m0_default(),
        "claude_code" => AdapterCapabilities::claude_code_default(),
        _ => AdapterCapabilities::default(),
    }
}

fn tmux_session_name(session_id: &str) -> String {
    let sanitized: String = session_id
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect();
    format!("llmparty_{sanitized}")
}

fn workspace_path(request: &RuntimeStartRequest) -> Result<PathBuf> {
    let path = request
        .workspace
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("llmparty-workspaces")
                .join(&request.session_id)
        });
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn runtime_dir(session_id: &str) -> Result<PathBuf> {
    Ok(llmparty_data_dir()?.join("runtimes").join(session_id))
}

fn llmparty_data_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("LLMPARTY_DATA_DIR") {
        return Ok(PathBuf::from(path));
    }

    let home = std::env::var("HOME").map_err(|_| Error::InvalidConfig {
        key: "HOME",
        message: "required to derive llmparty data directory".to_string(),
    })?;
    Ok(PathBuf::from(home).join(".local/share/llmparty"))
}

struct RuntimePaths<'a> {
    runtime_dir: &'a Path,
    log_path: &'a Path,
    adapter_event_log: &'a Path,
    current_turn_file: &'a Path,
    pi_hook_log: &'a Path,
    claude_hook_log: &'a Path,
}

fn write_runtime_script(
    path: &Path,
    workspace: &Path,
    runtime_paths: &RuntimePaths<'_>,
    request: &RuntimeStartRequest,
) -> Result<()> {
    let (log_setup, runtime_body) = match request.client_type.as_str() {
        "pi" => {
            let pi_command =
                std::env::var("LLMPARTY_PI_TUI_COMMAND").unwrap_or_else(|_| "pi".to_string());
            (
                "echo \"llmparty runtime started\" >> \"$LLMPARTY_RUNTIME_LOG\"".to_string(),
                format!("exec sh -lc {}\n", shell_quote(&pi_command)),
            )
        }
        "claude_code" => {
            let claude_command = std::env::var("LLMPARTY_CLAUDE_TUI_COMMAND")
                .unwrap_or_else(|_| "claude".to_string());
            (
                "echo \"llmparty runtime started\" >> \"$LLMPARTY_RUNTIME_LOG\"".to_string(),
                format!("exec sh -lc {}\n", shell_quote(&claude_command)),
            )
        }
        _ => (
            "exec >> \"$LLMPARTY_RUNTIME_LOG\" 2>&1\necho \"llmparty runtime started\"".to_string(),
            "trap 'exit 0' TERM INT\nwhile :; do sleep 60; done\n".to_string(),
        ),
    };
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
export LLMPARTY_PI_HOOK_LOG={}
export LLMPARTY_CLAUDE_HOOK_LOG={}
{}
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
        shell_quote(&runtime_paths.pi_hook_log.display().to_string()),
        shell_quote(&runtime_paths.claude_hook_log.display().to_string()),
        log_setup,
        runtime_body,
    );
    std::fs::write(path, content)?;
    Ok(())
}

fn internal_event_url() -> String {
    std::env::var("LLMPARTY_INTERNAL_EVENT_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8080/internal/v1/events".to_string())
}

fn sanitize_tmux_identifier(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

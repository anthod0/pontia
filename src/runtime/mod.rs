//! Runtime control boundary.
//!
//! The MVP generic runtime records a binding and immediately reports ready. This
//! module stays independent from HTTP transport details.

use std::{
    path::{Path, PathBuf},
    process::Command,
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
        let llmparty_dir = workspace.join(".llmparty");
        std::fs::create_dir_all(&llmparty_dir)?;
        let log_path = llmparty_dir.join("runtime.log");
        std::fs::File::create(&log_path)?;
        let script_path = llmparty_dir.join("runtime.sh");
        write_runtime_script(&script_path, &log_path, &request)?;

        let status = Command::new("tmux")
            .args([
                "new-session",
                "-d",
                "-s",
                &tmux_session,
                "-c",
                &workspace.display().to_string(),
                &format!("sh {}", shell_quote(&script_path.display().to_string())),
            ])
            .status()
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
        let log_path = log_path.display().to_string();
        Ok(RuntimeStartResult {
            runtime_kind: "tmux".to_string(),
            runtime_ref: tmux_session.clone(),
            capabilities: capabilities.into(),
            metadata: json!({
                "backend": "tmux",
                "tmux_session": tmux_session,
                "workspace": workspace,
                "log_path": log_path,
                "started_at": started_at,
                "restart_count": restart_count,
            }),
        })
    }

    pub fn submit_input(&self, input: AgentInput) -> Result<()> {
        GenericTestAdapter.accept_input(input)
    }

    pub fn terminate_session(&self, runtime_ref: &str) -> Result<()> {
        let status = Command::new("tmux")
            .args(["kill-session", "-t", runtime_ref])
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

fn capabilities_for_client(client_type: &str) -> AdapterCapabilities {
    match client_type {
        "generic" => GenericTestAdapter.capabilities(),
        "pi" => AdapterCapabilities::pi_m0_default(),
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
                .join("llmparty-runtimes")
                .join(&request.session_id)
        });
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

fn write_runtime_script(path: &Path, log_path: &Path, request: &RuntimeStartRequest) -> Result<()> {
    let workspace = path
        .parent()
        .and_then(|path| path.parent())
        .ok_or_else(|| Error::Domain("runtime script path missing workspace".to_string()))?;
    let content = format!(
        r#"#!/usr/bin/env sh
export LLMPARTY_SESSION_ID={}
export LLMPARTY_CLIENT_TYPE={}
export LLMPARTY_WORKSPACE={}
export LLMPARTY_RUNTIME_LOG={}
exec >> "$LLMPARTY_RUNTIME_LOG" 2>&1
echo "llmparty runtime started"
trap 'exit 0' TERM INT
while :; do sleep 60; done
"#,
        shell_quote(&request.session_id),
        shell_quote(&request.client_type),
        shell_quote(&workspace.display().to_string()),
        shell_quote(&log_path.display().to_string()),
    );
    std::fs::write(path, content)?;
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

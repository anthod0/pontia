//! Runtime control boundary.
//!
//! The MVP generic runtime records a binding and immediately reports ready. This
//! module stays independent from HTTP transport details.

mod claude_code;
mod config;
mod paths;
mod script;
mod tmux;

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::format_description::well_known::Rfc3339;

use crate::{
    adapters::{AdapterCapabilities, AgentEventSource, AgentInputSink, GenericTestAdapter},
    agent_clients::{
        self, AdapterEventBehavior, DispatchBehavior, InterruptBehavior, RuntimeBehavior,
    },
    application::SessionCapabilities,
    error::{Error, Result},
    ids::new_runtime_instance_id,
    time::utc_now,
};

pub use config::{set_runtime_config, set_runtime_external_api_token};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStartRequest {
    pub session_id: String,
    pub client_type: String,
    pub workspace: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub agent_kind: Option<String>,
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

#[derive(Debug, Clone)]
struct InProcessRuntimeState {
    alive: bool,
}

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
        let client_spec =
            agent_clients::get_client_spec(&request.client_type).ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", request.client_type))
            })?;
        let capabilities = if client_spec.dispatch == DispatchBehavior::GenericTestAdapter {
            GenericTestAdapter.capabilities()
        } else {
            client_spec.capabilities.clone()
        };
        if client_spec.runtime == RuntimeBehavior::InProcessTest {
            return self.start_in_process_test_session(request, capabilities, restart_count);
        }

        let tmux_session = tmux::tmux_session_name(&request);
        let workspace = paths::workspace_path(&request)?;
        script::run_startup_hooks(client_spec.startup_hooks, &workspace)?;
        let runtime_dir = paths::runtime_dir(&request.session_id)?;
        std::fs::create_dir_all(&runtime_dir)?;
        let log_path = runtime_dir.join("runtime.log");
        let adapter_event_log = match client_spec.adapter_events {
            AdapterEventBehavior::JsonlOutbox { file_name } => runtime_dir.join(file_name),
            AdapterEventBehavior::Disabled => runtime_dir.join("adapter-events.jsonl"),
        };
        let current_turn_file = runtime_dir.join("current-turn.json");
        let internal_event_url = script::internal_event_url();
        let runtime_instance_id = new_runtime_instance_id().to_string();
        std::fs::File::create(&log_path)?;
        let script_path = runtime_dir.join("runtime.sh");
        let runtime_paths = script::RuntimePaths {
            runtime_dir: &runtime_dir,
            log_path: &log_path,
            adapter_event_log: &adapter_event_log,
            current_turn_file: &current_turn_file,
        };
        script::write_runtime_script(
            &script_path,
            &workspace,
            &runtime_paths,
            &request,
            &runtime_instance_id,
        )?;

        let status = tmux::spawn_tmux_session(&tmux_session, &workspace, &script_path)
            .map_err(|err| Error::Domain(format!("tmux runtime spawn failed: {err}")))?;
        if !status.success() {
            return Err(Error::Domain(format!(
                "tmux runtime spawn failed with status {status}"
            )));
        }

        let started_at = utc_now()
            .format(&Rfc3339)
            .map_err(|err| Error::Domain(format!("invalid runtime timestamp: {err}")))?;
        let hook_log_metadata = client_spec
            .tmux_runtime()
            .and_then(|runtime| runtime.hook_log)
            .map(|hook_log| {
                (
                    hook_log.metadata_key,
                    runtime_dir.join(hook_log.file_name).display().to_string(),
                )
            });
        let workspace = workspace.display().to_string();
        let runtime_dir = runtime_dir.display().to_string();
        let log_path = log_path.display().to_string();
        let adapter_event_log = adapter_event_log.display().to_string();
        let current_turn_file = current_turn_file.display().to_string();
        let mut metadata = json!({
            "backend": "tmux",
            "tmux_session": tmux_session,
            "workspace": workspace,
            "runtime_dir": runtime_dir,
            "runtime_log": log_path,
            "log_path": log_path,
            "adapter_event_log": adapter_event_log,
            "current_turn_file": current_turn_file,
            "internal_event_url": internal_event_url,
            "handle": request.handle,
            "role": request.role,
            "started_at": started_at,
            "restart_count": restart_count,
            "runtime_instance_id": runtime_instance_id,
        });
        if let Some((metadata_key, path)) = hook_log_metadata
            && let Some(object) = metadata.as_object_mut()
        {
            object.insert(metadata_key.to_string(), json!(path));
        }
        Ok(RuntimeStartResult {
            runtime_kind: "tmux".to_string(),
            runtime_ref: tmux_session.clone(),
            capabilities: capabilities.into(),
            metadata,
        })
    }

    fn start_in_process_test_session(
        &self,
        request: RuntimeStartRequest,
        capabilities: AdapterCapabilities,
        restart_count: i64,
    ) -> Result<RuntimeStartResult> {
        let runtime_instance_id = new_runtime_instance_id().to_string();
        let started_at = utc_now()
            .format(&Rfc3339)
            .map_err(|err| Error::Domain(format!("invalid runtime timestamp: {err}")))?;
        let runtime_dir = std::env::temp_dir()
            .join("pilotfy-test-runtimes")
            .join(&request.session_id);
        std::fs::create_dir_all(&runtime_dir)?;
        let log_path = runtime_dir.join("runtime.log");
        let adapter_event_log = runtime_dir.join("adapter-events.jsonl");
        let current_turn_file = runtime_dir.join("current-turn.json");
        std::fs::File::create(&log_path)?;
        let runtime_dir = runtime_dir.display().to_string();
        let log_path = log_path.display().to_string();
        let adapter_event_log = adapter_event_log.display().to_string();
        let current_turn_file = current_turn_file.display().to_string();
        let runtime_ref = in_process_runtime_ref(&request);
        in_process_registry()
            .lock()
            .expect("in-process runtime registry lock")
            .insert(runtime_ref.clone(), InProcessRuntimeState { alive: true });
        Ok(RuntimeStartResult {
            runtime_kind: "in_process_test".to_string(),
            runtime_ref,
            capabilities: capabilities.into(),
            metadata: json!({
                "backend": "in_process_test",
                "test_runtime": true,
                "runtime_dir": runtime_dir,
                "runtime_log": log_path,
                "log_path": log_path,
                "adapter_event_log": adapter_event_log,
                "current_turn_file": current_turn_file,
                "internal_event_url": "in-process://internal-events",
                "handle": request.handle,
                "role": request.role,
                "started_at": started_at,
                "restart_count": restart_count,
                "runtime_instance_id": runtime_instance_id,
            }),
        })
    }

    pub fn submit_input(&self, input: AgentInput) -> Result<()> {
        GenericTestAdapter.accept_input(input)
    }

    pub fn dispatch_tui_turn(
        &self,
        runtime_ref: &str,
        client_type: &str,
        input: &AgentInput,
    ) -> Result<()> {
        tmux::dispatch_tui_turn(runtime_ref, client_type, input)
    }

    pub fn interrupt_session(&self, runtime_ref: &str, behavior: InterruptBehavior) -> Result<()> {
        match behavior {
            InterruptBehavior::Unsupported => Ok(()),
            InterruptBehavior::TmuxInterrupt => tmux::interrupt_session(runtime_ref),
        }
    }

    pub fn terminate_session(&self, runtime_ref: &str) -> Result<()> {
        if let Some(runtime) = in_process_registry()
            .lock()
            .expect("in-process runtime registry lock")
            .get_mut(runtime_ref)
        {
            runtime.alive = false;
            return Ok(());
        }
        tmux::terminate_session(runtime_ref)
    }

    pub fn restart_session(&self, request: RuntimeStartRequest) -> Result<RuntimeStartResult> {
        self.start_session(request)
    }

    pub fn is_alive(&self, runtime_ref: &str) -> bool {
        if let Some(runtime) = in_process_registry()
            .lock()
            .expect("in-process runtime registry lock")
            .get(runtime_ref)
        {
            return runtime.alive;
        }
        tmux::is_alive(runtime_ref)
    }

    pub fn reset_in_process_test_registry() {
        in_process_registry()
            .lock()
            .expect("in-process runtime registry lock")
            .clear();
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

fn in_process_registry() -> &'static Mutex<HashMap<String, InProcessRuntimeState>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, InProcessRuntimeState>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn in_process_runtime_ref(request: &RuntimeStartRequest) -> String {
    let handle = request
        .handle
        .as_deref()
        .map(|value| value.trim_start_matches('@'))
        .filter(|value| !value.is_empty())
        .map(sanitize_runtime_ref_part);
    let role = request
        .role
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(sanitize_runtime_ref_part);
    let Some(handle) = handle else {
        return format!("generic:{}", request.session_id);
    };
    let Some(role) = role else {
        return format!(
            "generic:{}:{}",
            handle,
            short_session_id(&request.session_id)
        );
    };
    format!(
        "generic:{}:{}:{}",
        handle,
        role,
        short_session_id(&request.session_id)
    )
}

fn short_session_id(session_id: &str) -> String {
    let id_body = session_id.rsplit('_').next().unwrap_or(session_id);
    let mut chars: Vec<char> = id_body.chars().rev().take(8).collect();
    chars.reverse();
    chars.into_iter().collect()
}

fn sanitize_runtime_ref_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_runtime_starts_in_process_without_tmux_backend() {
        let manager = GenericRuntimeManager;
        let session_id = "sess_generic_in_process".to_string();

        let runtime = manager
            .start_session(RuntimeStartRequest {
                session_id: session_id.clone(),
                client_type: "generic".to_string(),
                workspace: None,
                handle: None,
                role: None,
                agent_kind: None,
            })
            .expect("generic runtime should start");

        assert_eq!(runtime.runtime_kind, "in_process_test");
        assert_eq!(runtime.runtime_ref, format!("generic:{session_id}"));
        assert_eq!(runtime.metadata["backend"], "in_process_test");
        assert_eq!(runtime.metadata["test_runtime"], true);
        assert!(
            !runtime
                .metadata
                .as_object()
                .unwrap()
                .contains_key("tmux_session")
        );
    }

    #[test]
    fn generic_runtime_ref_uses_handle_role_and_short_session_id() {
        let manager = GenericRuntimeManager;
        let session_id = "sess_1234567890abcdef".to_string();

        let runtime = manager
            .start_session(RuntimeStartRequest {
                session_id,
                client_type: "generic".to_string(),
                workspace: None,
                handle: Some("@planner".to_string()),
                role: Some("execution reviewer".to_string()),
                agent_kind: None,
            })
            .expect("generic runtime should start");

        assert_eq!(
            runtime.runtime_ref,
            "generic:planner:execution_reviewer:90abcdef"
        );
    }

    #[test]
    fn generic_runtime_registry_tracks_lifecycle_and_restart_identity() {
        let manager = GenericRuntimeManager;
        let request = RuntimeStartRequest {
            session_id: "sess_runtime_lifecycle_abcdef12".to_string(),
            client_type: "generic".to_string(),
            workspace: None,
            handle: None,
            role: None,
            agent_kind: None,
        };

        let first = manager
            .start_session(request.clone())
            .expect("generic runtime should start");
        assert!(manager.is_alive(&first.runtime_ref));

        manager
            .terminate_session(&first.runtime_ref)
            .expect("terminate generic runtime");
        assert!(!manager.is_alive(&first.runtime_ref));

        let second = manager
            .start_session_with_restart_count(request, 1)
            .expect("generic runtime should restart");
        assert_eq!(second.runtime_ref, first.runtime_ref);
        assert!(manager.is_alive(&second.runtime_ref));
        assert_ne!(
            first.metadata["runtime_instance_id"],
            second.metadata["runtime_instance_id"]
        );
        assert_ne!(first.metadata["started_at"], second.metadata["started_at"]);
    }

    #[test]
    fn runtime_script_exports_pilotfy_agent_kind_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let runtime_dir = dir.path();
        let script_path = runtime_dir.join("runtime.sh");
        let paths = script::RuntimePaths {
            runtime_dir,
            log_path: &runtime_dir.join("runtime.log"),
            adapter_event_log: &runtime_dir.join("adapter-events.jsonl"),
            current_turn_file: &runtime_dir.join("current-turn.json"),
        };

        script::write_runtime_script(
            &script_path,
            runtime_dir,
            &paths,
            &RuntimeStartRequest {
                session_id: "sess_planner".to_string(),
                client_type: "pi".to_string(),
                workspace: Some(runtime_dir.display().to_string()),
                handle: None,
                role: None,
                agent_kind: Some("planner".to_string()),
            },
            "rtinst_1",
        )
        .expect("write runtime script");

        let content = std::fs::read_to_string(script_path).expect("runtime script");
        assert!(content.contains("export PILOTFY_AGENT_KIND='planner'"));
    }
}

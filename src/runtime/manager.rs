use serde_json::json;
use time::format_description::well_known::Rfc3339;

use crate::{
    adapters::{AgentEventSource, AgentInputSink, GenericTestAdapter},
    agent_clients::{
        self, AdapterEventBehavior, DispatchBehavior, InterruptBehavior, RuntimeBehavior,
    },
    error::{Error, Result},
    ids::new_runtime_instance_id,
    time::utc_now,
};

use super::{AgentInput, RuntimeStartRequest, RuntimeStartResult, in_process, paths, script, tmux};

#[derive(Debug, Clone, Default)]
pub struct GenericRuntimeManager;

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
            return in_process::start_session(request, capabilities, restart_count);
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
            "tmux": {
                "session_name": tmux_session,
            },
            "workspace": workspace,
            "launch_cwd": workspace,
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
        if in_process::terminate_session(runtime_ref) {
            return Ok(());
        }
        tmux::terminate_session(runtime_ref)
    }

    pub fn restart_session(&self, request: RuntimeStartRequest) -> Result<RuntimeStartResult> {
        self.start_session(request)
    }

    pub fn is_alive(&self, runtime_ref: &str) -> bool {
        if let Some(alive) = in_process::is_alive(runtime_ref) {
            return alive;
        }
        tmux::is_alive(runtime_ref)
    }

    pub fn reset_in_process_test_registry() {
        in_process::reset_registry();
    }
}

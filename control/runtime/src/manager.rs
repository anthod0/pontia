use serde_json::json;
use time::format_description::well_known::Rfc3339;

use pontia_agent_clients::{
    self as agent_clients, AdapterEventBehavior, InterruptBehavior, RuntimeBehavior,
};
use pontia_core::{
    error::{Error, Result},
    ids::new_runtime_instance_id,
    time::utc_now,
};

use super::{
    AgentInput, RuntimeStartRequest, RuntimeStartResult, config, in_process, paths, script, tmux,
};

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
        self.start_session_with_restart_count_and_reuse_target(request, restart_count, None)
    }

    pub fn start_session_with_restart_count_and_reuse_target(
        &self,
        request: RuntimeStartRequest,
        restart_count: i64,
        reuse_target: Option<(&str, &str)>,
    ) -> Result<RuntimeStartResult> {
        let client_spec =
            agent_clients::get_client_spec(&request.client_type).ok_or_else(|| {
                Error::Domain(format!("unsupported client_type: {}", request.client_type))
            })?;
        let capabilities = if client_spec.adapter.runtime == RuntimeBehavior::InProcess {
            agent_clients::in_process_capabilities(&request.client_type)
                .unwrap_or_else(|| client_spec.capabilities.clone())
        } else {
            client_spec.capabilities.clone()
        };
        if client_spec.adapter.runtime == RuntimeBehavior::InProcess {
            return in_process::start_session(request, capabilities, restart_count);
        }

        let start_command = request.start_command.clone().or_else(|| {
            client_spec.tmux_runtime().map(|runtime| {
                runtime
                    .command_env
                    .and_then(|env| std::env::var(env).ok())
                    .or_else(|| config::configured_tui_command(&request.client_type))
                    .unwrap_or_else(|| {
                        let mut command = runtime.default_command.to_string();
                        if let Some(session_identity_arg) = runtime.session_identity_arg {
                            command.push(' ');
                            command.push_str(session_identity_arg);
                            command.push(' ');
                            command.push_str(&request.session_id);
                        }
                        command
                    })
            })
        });
        let base_tmux_session = tmux::tmux_session_name(&request);
        let reuse_target = reuse_target.filter(|(socket_path, pane_id)| {
            tmux::is_reusable_pontia_shell_pane(socket_path, pane_id, &request.session_id)
        });
        let tmux_session = if reuse_target.is_some() {
            base_tmux_session.clone()
        } else if restart_count > 0 && tmux::is_alive(&base_tmux_session) {
            format!("{base_tmux_session}_r{restart_count}")
        } else {
            base_tmux_session
        };
        let workspace = paths::workspace_path(&request)?;
        script::run_startup_hooks(client_spec.adapter.startup_hooks, &workspace)?;
        let runtime_dir = paths::runtime_dir(&request.session_id)?;
        std::fs::create_dir_all(&runtime_dir)?;
        let log_path = runtime_dir.join("runtime.log");
        let adapter_event_log = match client_spec.adapter.adapter_events {
            AdapterEventBehavior::JsonlOutbox { file_name } => runtime_dir.join(file_name),
            AdapterEventBehavior::Disabled => runtime_dir.join("adapter-events.jsonl"),
        };
        let internal_event_url = script::internal_event_url();
        let runtime_instance_id = new_runtime_instance_id().to_string();
        std::fs::File::create(&log_path)?;
        let runtime_paths = script::RuntimePaths {
            runtime_dir: &runtime_dir,
            log_path: &log_path,
            adapter_event_log: &adapter_event_log,
        };
        let launch_script_path = script::write_ephemeral_launch_script(
            &workspace,
            &runtime_paths,
            &request,
            &runtime_instance_id,
        )?;
        let quoted_launch_script_path = script::shell_quote_path(&launch_script_path);
        let launch_command =
            format!("sh {quoted_launch_script_path}; rm -f {quoted_launch_script_path}");

        let pane_binding = if let Some((socket_path, pane_id)) = reuse_target {
            tmux::run_launch_command_in_pane(socket_path, pane_id, &launch_command)?;
            Some(tmux::TmuxPaneBinding {
                socket_path: socket_path.to_string(),
                pane_id: pane_id.to_string(),
            })
        } else {
            let status = tmux::spawn_tmux_session(&tmux_session, &workspace, &launch_command)
                .map_err(|err| Error::Domain(format!("tmux runtime spawn failed: {err}")))?;
            if !status.success() {
                return Err(Error::Domain(format!(
                    "tmux runtime spawn failed with status {status}"
                )));
            }
            tmux::pane_binding(&tmux_session)
        };
        if let Some(binding) = pane_binding.as_ref() {
            tmux::mark_pontia_pane(
                &binding.socket_path,
                &binding.pane_id,
                &request.session_id,
                &runtime_instance_id,
            )?;
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
            "internal_event_url": internal_event_url,
            "handle": request.handle,
            "role": request.role,
            "started_at": started_at,
            "restart_count": restart_count,
            "runtime_instance_id": runtime_instance_id,
            "start_command": start_command,
        });
        if let Some(binding) = pane_binding
            && let Some(object) = metadata.as_object_mut()
        {
            object.insert("tmux_socket_path".to_string(), json!(binding.socket_path));
            object.insert("tmux_pane_id".to_string(), json!(binding.pane_id));
        }
        if let Some((metadata_key, path)) = hook_log_metadata
            && let Some(object) = metadata.as_object_mut()
        {
            object.insert(metadata_key.to_string(), json!(path));
        }
        Ok(RuntimeStartResult {
            runtime_kind: "tmux".to_string(),
            runtime_handle: tmux_session.clone(),
            capabilities,
            metadata,
        })
    }

    pub fn submit_input(&self, client_type: &str, input: AgentInput) -> Result<()> {
        agent_clients::accept_in_process_input(client_type, input)
    }

    pub fn dispatch_tui_turn(
        &self,
        socket_path: &str,
        pane_id: &str,
        client_type: &str,
        input: &AgentInput,
    ) -> Result<()> {
        tmux::dispatch_tui_turn(socket_path, pane_id, client_type, input)
    }

    pub fn interrupt_session(
        &self,
        socket_path: &str,
        pane_id: &str,
        behavior: InterruptBehavior,
    ) -> Result<()> {
        match behavior {
            InterruptBehavior::Unsupported => Ok(()),
            InterruptBehavior::TmuxInterrupt => tmux::interrupt_session(socket_path, pane_id),
        }
    }

    pub fn terminate_session(&self, runtime_handle: &str) -> Result<()> {
        if in_process::terminate_session(runtime_handle) {
            return Ok(());
        }
        tmux::terminate_session(runtime_handle)
    }

    pub fn send_tmux_keys(&self, socket_path: &str, pane_id: &str, keys: &[&str]) -> Result<()> {
        tmux::send_keys(socket_path, pane_id, keys)
    }

    pub fn mark_tmux_pane_for_session(
        &self,
        socket_path: &str,
        pane_id: &str,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        tmux::mark_pontia_pane(socket_path, pane_id, session_id, runtime_instance_id)
    }

    pub fn kill_tmux_pane(&self, socket_path: &str, pane_id: &str) -> Result<()> {
        tmux::kill_pane(socket_path, pane_id)
    }

    pub fn terminate_tmux_pane(
        &self,
        socket_path: &str,
        pane_id: &str,
        keys: &[&str],
    ) -> Result<()> {
        if tmux::send_keys(socket_path, pane_id, keys).is_err() {
            tmux::kill_pane(socket_path, pane_id)?;
        }
        Ok(())
    }

    pub fn is_tmux_pane_alive(&self, socket_path: &str, pane_id: &str) -> bool {
        tmux::is_pane_alive(socket_path, pane_id)
    }

    pub fn restart_session(&self, request: RuntimeStartRequest) -> Result<RuntimeStartResult> {
        self.start_session(request)
    }

    pub fn is_alive(&self, runtime_handle: &str) -> bool {
        if let Some(alive) = in_process::is_alive(runtime_handle) {
            return alive;
        }
        tmux::is_alive(runtime_handle)
    }

    pub fn reset_in_process_registry() {
        in_process::reset_registry();
    }
}

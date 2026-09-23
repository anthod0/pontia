use std::path::Path;

use serde_json::json;
use time::format_description::well_known::Rfc3339;

use crate::TmuxLaunchOptions;
use pontia_core::{
    error::{Error, Result},
    ids::{new_event_id, new_runtime_instance_id},
    time::utc_now,
};

use super::{RuntimeStartRequest, RuntimeStartResult, in_process, paths, script, tmux};

#[derive(Debug, Clone, Default)]
pub struct GenericRuntimeManager;

impl GenericRuntimeManager {
    pub fn start_in_process(
        &self,
        root: &Path,
        request: RuntimeStartRequest,
        capabilities: pontia_core::client_capabilities::AgentClientCapabilities,
        count: i64,
    ) -> Result<RuntimeStartResult> {
        in_process::start_session(root, request, capabilities, count)
    }

    pub fn start_tmux(
        &self,
        pontia_home: &Path,
        request: RuntimeStartRequest,
        restart_count: i64,
        reuse_target: Option<(&str, &str)>,
        options: &TmuxLaunchOptions,
    ) -> Result<RuntimeStartResult> {
        let capabilities = options.capabilities.clone();
        let start_command = request.start_command.clone();
        let base_tmux_session = tmux::tmux_session_name(&request);
        let reuse_target = reuse_target
            .filter(|(socket_path, pane_id)| tmux::is_reusable_shell_pane(socket_path, pane_id));
        let tmux_session = if reuse_target.is_some() {
            base_tmux_session.clone()
        } else if restart_count > 0 && tmux::is_alive(&base_tmux_session) {
            format!("{base_tmux_session}_r{restart_count}")
        } else {
            base_tmux_session
        };
        let workspace = paths::workspace_path(pontia_home, &request)?;
        let log_paths = paths::log_paths(pontia_home);
        std::fs::create_dir_all(&log_paths.log_dir)?;
        let log_path = log_paths.runtime_log.clone();
        let launch_id = format!("launch_{}", new_event_id());
        let runtime_instance_id = new_runtime_instance_id().to_string();
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        let runtime_paths = script::RuntimePaths {
            log_path: &log_path,
        };
        let launch_script_path = script::write_ephemeral_launch_script(
            pontia_home,
            &runtime_paths,
            &request,
            &launch_id,
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
        let started_at = utc_now()
            .format(&Rfc3339)
            .map_err(|err| Error::Domain(format!("invalid runtime timestamp: {err}")))?;
        let hook_log_metadata = options
            .hook_log
            .map(|(file, key)| (key, log_paths.client_hook_log(file).display().to_string()));
        let workspace = workspace.display().to_string();
        let log_dir = log_paths.log_dir.display().to_string();
        let log_path = log_path.display().to_string();
        let mut metadata = json!({
            "backend": "tmux",
            "tmux_session": tmux_session,
            "tmux": {
                "session_name": tmux_session,
            },
            "workspace": workspace,
            "launch_cwd": workspace,
            "log_dir": log_dir,
            "runtime_log": log_path,
            "log_path": log_path,
            "handle": request.handle,
            "role": request.role,
            "started_at": started_at,
            "restart_count": restart_count,
            "launch_id": launch_id,
            "runtime_instance_id": runtime_instance_id,
            "binding_confirmed": false,
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
            object.insert(
                "client_diagnostics".to_string(),
                json!({metadata_key: path}),
            );
        }
        Ok(RuntimeStartResult {
            runtime_kind: "tmux".to_string(),
            runtime_handle: tmux_session.clone(),
            capabilities,
            metadata,
        })
    }

    pub fn terminate_session(&self, runtime_handle: &str) -> Result<()> {
        if in_process::terminate_session(runtime_handle) {
            return Ok(());
        }
        tmux::terminate_session(runtime_handle)
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

    pub fn clear_tmux_pane_markers(
        &self,
        socket_path: &str,
        pane_id: &str,
        expected_session_id: &str,
        expected_runtime_instance_id: &str,
    ) -> Result<()> {
        tmux::clear_pontia_pane_markers(
            socket_path,
            pane_id,
            expected_session_id,
            expected_runtime_instance_id,
        )
    }

    pub fn kill_tmux_pane(&self, socket_path: &str, pane_id: &str) -> Result<()> {
        tmux::kill_pane(socket_path, pane_id)
    }

    pub fn is_tmux_pane_alive(&self, socket_path: &str, pane_id: &str) -> bool {
        tmux::is_pane_alive(socket_path, pane_id)
    }

    pub fn capture_tmux_process_fingerprint(
        &self,
        socket_path: &str,
        pane_id: &str,
        process_names: &[&str],
    ) -> Option<crate::TmuxProcessFingerprint> {
        tmux::capture_fingerprint(socket_path, pane_id, process_names)
    }

    pub fn validate_tmux_process_fingerprint(
        &self,
        socket_path: &str,
        pane_id: &str,
        fingerprint: &crate::TmuxProcessFingerprint,
    ) -> bool {
        tmux::validate_fingerprint(socket_path, pane_id, fingerprint)
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

//! Runtime control boundary.
//!
//! The MVP generic runtime records a binding and immediately reports ready. This
//! module stays independent from HTTP transport details.

mod config;
mod in_process;
mod manager;
mod paths;
mod script;
mod tmux;
mod types;
mod utils;

#[cfg(test)]
pub use config::reset_runtime_bind_addr_for_tests;
pub use config::{
    configured_internal_event_url, set_runtime_bind_addr, set_runtime_config,
    set_runtime_external_api_token,
};
pub use manager::GenericRuntimeManager;
pub use types::{AgentInput, RuntimeStartRequest, RuntimeStartResult};

#[cfg(test)]
mod tests {
    use std::{
        process::{Command, Stdio},
        thread,
        time::Duration,
    };

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
                start_command: None,
            })
            .expect("generic runtime should start");

        assert_eq!(runtime.runtime_kind, "in_process");
        assert_eq!(runtime.runtime_handle, format!("generic:{session_id}"));
        assert_eq!(runtime.metadata["backend"], "in_process");
        assert_eq!(runtime.metadata["in_process_runtime"], true);
        assert!(
            !runtime
                .metadata
                .as_object()
                .unwrap()
                .contains_key("tmux_session")
        );
    }

    #[test]
    fn generic_runtime_handle_uses_handle_role_and_short_session_id() {
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
                start_command: None,
            })
            .expect("generic runtime should start");

        assert_eq!(
            runtime.runtime_handle,
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
            start_command: None,
        };

        let first = manager
            .start_session(request.clone())
            .expect("generic runtime should start");
        assert!(manager.is_alive(&first.runtime_handle));

        manager
            .terminate_session(&first.runtime_handle)
            .expect("terminate generic runtime");
        assert!(!manager.is_alive(&first.runtime_handle));

        let second = manager
            .start_session_with_restart_count(request, 1)
            .expect("generic runtime should restart");
        assert_eq!(second.runtime_handle, first.runtime_handle);
        assert!(manager.is_alive(&second.runtime_handle));
        assert_ne!(
            first.metadata["runtime_instance_id"],
            second.metadata["runtime_instance_id"]
        );
        assert_ne!(first.metadata["started_at"], second.metadata["started_at"]);
    }

    #[test]
    fn tmux_runtime_reuses_marked_shell_pane_when_requested() {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("reused.log");
        let session_id = "sess_tmux_reuse_shell".to_string();
        let tmux_session = format!("pontia_test_reuse_manager_{}", std::process::id());
        let status = Command::new("tmux")
            .args(["new-session", "-d", "-s", &tmux_session, "sh"])
            .stderr(Stdio::null())
            .status()
            .expect("spawn tmux");
        assert!(status.success(), "tmux session should start");
        let binding = tmux::pane_binding(&tmux_session).expect("pane binding");
        tmux::mark_pontia_pane(
            &binding.socket_path,
            &binding.pane_id,
            &session_id,
            "rtinst_previous",
        )
        .expect("mark pane");
        for _ in 0..50 {
            if tmux::is_reusable_pontia_shell_pane(
                &binding.socket_path,
                &binding.pane_id,
                &session_id,
            ) {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(tmux::is_reusable_pontia_shell_pane(
            &binding.socket_path,
            &binding.pane_id,
            &session_id,
        ));

        let manager = GenericRuntimeManager;
        let runtime = manager
            .start_session_with_restart_count_and_reuse_target(
                RuntimeStartRequest {
                    session_id: session_id.clone(),
                    client_type: "pi".to_string(),
                    workspace: Some(dir.path().display().to_string()),
                    handle: None,
                    role: None,
                    agent_kind: None,
                    start_command: Some(format!("printf reused > {}", output.display())),
                },
                1,
                Some((&binding.socket_path, &binding.pane_id)),
            )
            .expect("start by reusing pane");

        assert_eq!(runtime.tmux_pane_id(), Some(binding.pane_id.as_str()));
        for _ in 0..50 {
            if output.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = Command::new("tmux")
            .args(["kill-session", "-t", &tmux_session])
            .stderr(Stdio::null())
            .status();
        assert_eq!(std::fs::read_to_string(output).expect("output"), "reused");
    }

    #[test]
    fn runtime_script_exports_pontia_agent_kind_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let runtime_dir = dir.path();
        let script_path = runtime_dir.join("launch.sh");
        let paths = script::RuntimePaths {
            runtime_dir,
            log_path: &runtime_dir.join("runtime.log"),
            adapter_event_log: &runtime_dir.join("adapter-events.jsonl"),
        };

        script::write_launch_script(
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
                start_command: None,
            },
            "rtinst_1",
        )
        .expect("write launch script");

        let content = std::fs::read_to_string(script_path).expect("launch script");
        assert!(content.contains("export PONTIA_AGENT_KIND='planner'"));
    }
}

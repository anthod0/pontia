//! Runtime control boundary.
//!
//! The MVP generic runtime records a binding and immediately reports ready. This
//! module stays independent from HTTP transport details.

mod claude_code;
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
    fn runtime_script_exports_pontia_agent_kind_when_present() {
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
        assert!(content.contains("export PONTIA_AGENT_KIND='planner'"));
    }
}

use pontia_runtime::{GenericRuntimeManager, RuntimeStartRequest};

#[test]
fn runtime_crate_exposes_manager_and_start_types() {
    let manager = GenericRuntimeManager;
    let runtime = manager
        .start_session(RuntimeStartRequest {
            session_id: "sess_runtime_boundary".to_string(),
            client_type: "generic".to_string(),
            workspace: None,
            workspace_name: None,
            handle: None,
            role: None,
            start_command: None,
        })
        .expect("generic runtime starts");

    assert_eq!(runtime.runtime_kind, "in_process");
    assert_eq!(runtime.runtime_handle, "generic:sess_runtime_boundary");
}

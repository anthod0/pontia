use crate::RuntimeStartRequest;

use super::super::tmux_session_name;

#[test]
fn tmux_session_name_includes_workspace_name_and_short_session_id() {
    let name = tmux_session_name(&RuntimeStartRequest {
        session_id: "sess_1234567890abcdef".to_string(),
        client_type: "pi".to_string(),
        workspace: Some("/repo/ignored-path-name".to_string()),
        workspace_name: Some("Pontia App".to_string()),
        handle: Some("@main".to_string()),
        role: Some("coder".to_string()),
        start_command: None,
        environment: Default::default(),
    });

    assert_eq!(name, "pontia_Pontia_App_main_coder_90abcdef");
}

#[test]
fn tmux_session_name_falls_back_to_workspace_basename_and_never_uses_full_session_id() {
    let name = tmux_session_name(&RuntimeStartRequest {
        session_id: "sess_1234567890abcdef".to_string(),
        client_type: "pi".to_string(),
        workspace: Some("/repo/pontia".to_string()),
        workspace_name: None,
        handle: None,
        role: None,
        start_command: None,
        environment: Default::default(),
    });

    assert_eq!(name, "pontia_pontia_90abcdef");
}

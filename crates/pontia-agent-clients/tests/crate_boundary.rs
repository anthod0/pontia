#[test]
fn native_identity_requirements_are_declared_by_the_client() {
    assert!(pontia_agent_clients::client_session_identity_required_on_ready("codex"));
    assert!(!pontia_agent_clients::client_session_identity_required_on_ready("generic"));
    assert!(!pontia_agent_clients::client_session_identity_required_on_ready("unknown"));
}

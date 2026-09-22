#[test]
fn agent_clients_crate_answers_client_boundary_questions() {
    assert_eq!(
        pontia_agent_clients::default_real_client_spec().client_type,
        "pi"
    );
    assert_eq!(pontia_agent_clients::default_real_client_type(), "pi");
    assert!(pontia_agent_clients::client_session_identity_required_on_ready("pi"));
    assert!(!pontia_agent_clients::client_session_identity_required_on_ready("generic"));
    assert!(!pontia_agent_clients::client_session_identity_required_on_ready("unknown"));
}

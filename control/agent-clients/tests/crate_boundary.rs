use pontia_agent_clients::get_client_spec;

#[test]
fn agent_clients_crate_owns_pi_timeline_item_detail_backend() {
    let spec = get_client_spec("pi").expect("pi spec");
    assert!(spec.capabilities.timeline);

    let backend = pontia_agent_clients::timeline_item_detail_backend_for("pi")
        .expect("pi timeline item detail backend");
    assert_eq!(backend.reader.client_type(), "pi");
    assert_eq!(backend.reader.format(), "pi-jsonl");
}

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

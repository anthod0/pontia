use pontia_agent_clients::get_client_spec;

#[test]
fn agent_clients_crate_owns_pi_raw_transcript_backend() {
    let spec = get_client_spec("pi").expect("pi spec");
    assert!(spec.capabilities.timeline);

    let backend = pontia_agent_clients::raw_transcript_backend_for("pi")
        .expect("pi transcript backend");
    assert_eq!(backend.parser.client_type(), "pi");
    assert_eq!(backend.parser.format(), "pi-jsonl");
}

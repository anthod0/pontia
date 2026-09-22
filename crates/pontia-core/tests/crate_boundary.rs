use pontia_core::{domain::SessionState, ids};

#[test]
fn core_crate_exposes_domain_state_and_id_generation() {
    assert!(SessionState::Created.to_string().contains("created"));
    assert!(ids::new_session_id().to_string().starts_with("sess_"));
}

use pontia_core::{ids, time};

#[test]
fn generated_ids_have_external_prefixes_and_are_unique() {
    let session_id = ids::new_session_id();
    let turn_id = ids::new_turn_id();
    let event_id = ids::new_event_id();
    let dispatch_id = ids::new_dispatch_id();
    let another_session_id = ids::new_session_id();

    assert!(session_id.as_str().starts_with("sess_"));
    assert!(turn_id.as_str().starts_with("turn_"));
    assert!(event_id.as_str().starts_with("evt_"));
    assert!(dispatch_id.as_str().starts_with("dispatch_"));
    assert_ne!(session_id, another_session_id);
}

#[test]
fn utc_now_returns_offset_datetime_in_utc() {
    let now = time::utc_now();

    assert_eq!(now.offset(), ::time::UtcOffset::UTC);
}

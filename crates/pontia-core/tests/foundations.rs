use std::str::FromStr;

use pontia_core::{
    domain::{DomainEvent, EventSource, EventType, ProjectionState},
    ids, time,
};
use serde_json::json;

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

#[test]
fn runtime_event_types_round_trip_through_text_and_json() {
    for (event_type, name) in [
        (EventType::RuntimeStarting, "runtime.starting"),
        (EventType::RuntimeReady, "runtime.ready"),
        (EventType::RuntimeExited, "runtime.exited"),
    ] {
        assert_eq!(event_type.to_string(), name);
        assert_eq!(EventType::from_str(name).unwrap(), event_type);
        assert_eq!(serde_json::to_value(event_type).unwrap(), name);
        assert_eq!(
            serde_json::from_value::<EventType>(json!(name)).unwrap(),
            event_type
        );
    }
}

#[test]
fn runtime_events_require_a_runtime_id_and_forbid_a_turn_id() {
    let event = |turn_id: Option<&str>, payload| {
        DomainEvent::new(
            "evt_runtime_shape".to_string(),
            "sess_runtime_shape".to_string(),
            turn_id.map(str::to_string),
            EventSource::RuntimeManager,
            "generic".to_string(),
            EventType::RuntimeReady,
            payload,
        )
    };

    assert!(
        ProjectionState::default()
            .apply(&event(None, json!({})))
            .is_err()
    );
    assert!(
        ProjectionState::default()
            .apply(&event(
                Some("turn_invalid"),
                json!({"runtime_id":"runtime"})
            ))
            .is_err()
    );
    assert!(
        ProjectionState::default()
            .apply(&event(None, json!({"runtime_id":"runtime"})))
            .is_ok()
    );
}

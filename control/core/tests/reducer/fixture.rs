use pontia_core::domain::{DomainEvent, EventSource, EventType};
use serde_json::json;

pub(super) fn event(event_type: EventType, session_id: &str, turn_id: Option<&str>) -> DomainEvent {
    DomainEvent::new(
        format!("evt_{:?}_{:?}", event_type, turn_id).replace(['.', '"', ' '], "_"),
        session_id.to_string(),
        turn_id.map(str::to_string),
        EventSource::ExternalApi,
        "generic".to_string(),
        event_type,
        json!({}),
    )
}

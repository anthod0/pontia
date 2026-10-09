use pontia_application::client_contract::ClientEventInterpreter;
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};

pub(crate) struct PiEventInterpreter;

impl ClientEventInterpreter for PiEventInterpreter {
    fn accompanying_runtime_event(&self, event: &DomainEvent) -> Result<Option<DomainEvent>> {
        let event_type = match event.event_type {
            EventType::SessionStarting | EventType::SessionResuming => EventType::RuntimeStarting,
            EventType::SessionReady => EventType::RuntimeReady,
            EventType::SessionExited => EventType::RuntimeExited,
            EventType::SessionError
                if event.payload["reason"] == "startup_timeout"
                    || event.payload["reason"] == "startup_failed" =>
            {
                EventType::RuntimeExited
            }
            _ => return Ok(None),
        };
        let runtime_id = event.payload["runtime_id"]
            .as_str()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| Error::StateConflict("Pi lifecycle event requires runtime_id".into()))?;
        let mut payload = event.payload.clone();
        if !payload.is_object() {
            payload = serde_json::json!({});
        }
        payload["runtime_id"] = serde_json::json!(runtime_id);
        payload["session_event_id"] = serde_json::json!(event.event_id);
        Ok(Some(DomainEvent {
            event_id: format!("{}:runtime", event.event_id),
            session_id: event.session_id.clone(),
            turn_id: None,
            source: event.source,
            client_type: event.client_type.clone(),
            event_type,
            occurred_at: event.occurred_at,
            payload,
            timeline_boundary: None,
            topology: None,
        }))
    }
}

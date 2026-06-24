use serde_json::Value;

use pontia_core::{
    domain::{DomainEvent, EventSource, EventType},
    error::{Error, Result},
};

#[derive(Clone, Default)]
pub struct InternalEventValidationService;

impl InternalEventValidationService {
    pub fn new() -> Self {
        Self
    }

    pub fn validate(&self, event: &DomainEvent) -> Result<()> {
        if event.event_type == EventType::SessionReady && event.source == EventSource::AgentClient {
            let runtime_instance_id = event
                .payload
                .get("runtime_instance_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if runtime_instance_id.trim().is_empty() {
                return Err(Error::Domain(
                    "session.ready from agent_client requires payload.runtime_instance_id"
                        .to_string(),
                ));
            }
            if pontia_agent_clients::client_session_identity_required_on_ready(&event.client_type) {
                let client_session_key = event
                    .payload
                    .get("client_session_key")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if client_session_key.trim().is_empty() {
                    return Err(Error::Domain(format!(
                        "{} session.ready from agent_client requires payload.client_session_key",
                        event.client_type
                    )));
                }
            }
        }

        Ok(())
    }
}

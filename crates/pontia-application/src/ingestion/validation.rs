use serde_json::Value;

use pontia_core::{
    domain::{DomainEvent, EventSource, EventType},
    error::{Error, Result},
};

#[derive(Clone, Default)]
pub struct InternalEventValidationService {
    clients: crate::clients::ClientRegistry,
}

impl InternalEventValidationService {
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub fn new() -> Self {
        Self::default()
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
            if self.clients.spec(&event.client_type).is_some_and(|spec| {
                spec.adapter.client_session_identity
                    == crate::client_contract::ClientSessionIdentityBehavior::RequiredOnReady
            }) {
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

        if event.event_type == EventType::SessionModelUpdated
            && !event
                .payload
                .get("model")
                .and_then(Value::as_str)
                .is_some_and(|model| !model.trim().is_empty())
        {
            return Err(Error::Domain(
                "payload.model must be a non-empty string".into(),
            ));
        }
        Ok(())
    }
}

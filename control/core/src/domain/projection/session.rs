use serde_json::{Value, json};

use super::{ProjectionState, SessionProjection};
use crate::{
    domain::{DomainEvent, EventType, SessionState},
    error::{Error, Result},
};

impl ProjectionState {
    pub(super) fn apply_session(&mut self, event: &DomainEvent, state: SessionState) -> Result<()> {
        let session = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| SessionProjection {
                session_id: event.session_id.clone(),
                client_type: event.client_type.clone(),
                title: None,
                handle: None,
                role: None,
                description: None,
                execution_profile_id: None,
                execution_profile_version: None,
                state: SessionState::Created,
                current_turn_id: None,
                state_version: 0,
                metadata: Value::Object(Default::default()),
            });

        session.state = state;
        if event.event_type == EventType::SessionCreated {
            session.title = event
                .payload
                .get("title")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.handle = event
                .payload
                .get("handle")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.role = event
                .payload
                .get("role")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.description = event
                .payload
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.execution_profile_id = event
                .payload
                .get("execution_profile_id")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.execution_profile_version = event
                .payload
                .get("execution_profile_version")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            if let Some(metadata) = event.payload.get("metadata") {
                session.metadata = metadata.clone();
            }
        }
        if event.event_type == EventType::SessionTitleUpdated {
            session.title = event
                .payload
                .get("title")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        session.state_version += 1;
        Ok(())
    }

    pub(super) fn apply_context_usage(&mut self, event: &DomainEvent) -> Result<()> {
        let Some(session) = self.sessions.get_mut(&event.session_id) else {
            return Ok(());
        };
        let usage = event
            .payload
            .get("context_usage")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                Error::Domain("payload.context_usage must be a JSON object".to_string())
            })?;
        let observed_at = event
            .occurred_at
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|err| Error::Domain(format!("invalid event timestamp: {err}")))?;
        let has_observed_usage = [
            "used_tokens",
            "max_tokens",
            "remaining_tokens",
            "usage_ratio",
            "input_tokens",
            "output_tokens",
            "cache_tokens",
        ]
        .iter()
        .any(|field| usage.get(*field).is_some_and(|value| !value.is_null()));

        if !session.metadata.is_object() {
            session.metadata = json!({});
        }
        if let Some(metadata) = session.metadata.as_object_mut() {
            let existing = metadata
                .get("context_usage")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            if has_observed_usage || !existing.is_empty() {
                let mut context_usage = serde_json::Map::new();
                for field in [
                    "used_tokens",
                    "max_tokens",
                    "remaining_tokens",
                    "usage_ratio",
                    "input_tokens",
                    "output_tokens",
                    "cache_tokens",
                ] {
                    let value = usage
                        .get(field)
                        .filter(|value| !value.is_null())
                        .cloned()
                        .or_else(|| existing.get(field).cloned())
                        .unwrap_or(Value::Null);
                    context_usage.insert(field.to_string(), value);
                }
                let confidence = usage
                    .get("confidence")
                    .filter(|value| !value.is_null())
                    .cloned()
                    .or_else(|| existing.get("confidence").cloned())
                    .unwrap_or_else(|| json!("unknown"));
                context_usage.insert("confidence".to_string(), confidence);
                context_usage.insert("observed_at".to_string(), json!(observed_at));
                metadata.insert("context_usage".to_string(), Value::Object(context_usage));
            }
            if let Some(model) = event.payload.get("model").filter(|model| !model.is_null()) {
                metadata.insert("model".to_string(), model.clone());
            }
        }
        session.state_version += 1;
        Ok(())
    }
}

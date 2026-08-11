use serde_json::{Value, json};

use super::ProjectionState;
use crate::{
    domain::DomainEvent,
    error::{Error, Result},
};

impl ProjectionState {
    pub(super) fn apply_approval_requested(&mut self, event: &DomainEvent) -> Result<()> {
        let turn_id = event.turn_id.as_deref().expect("validated turn_id");
        let Some(turn) = self.turns.get(turn_id) else {
            return Err(Error::Domain(format!(
                "approval request {} references missing turn {turn_id}",
                event.event_id
            )));
        };
        if turn.session_id != event.session_id || !turn.state.is_active() {
            return Err(Error::Domain(format!(
                "approval request {} must reference the active Turn",
                event.event_id
            )));
        }
        let session = self.sessions.get_mut(&event.session_id).ok_or_else(|| {
            Error::Domain(format!(
                "approval request {} references missing session {}",
                event.event_id, event.session_id
            ))
        })?;
        if !session.metadata.is_object() {
            session.metadata = json!({});
        }
        session
            .metadata
            .as_object_mut()
            .expect("metadata normalized")
            .insert(
                "interaction".to_string(),
                json!({
                    "type": "approval",
                    "state": "awaiting",
                    "request_event_id": event.event_id,
                }),
            );
        session.state_version += 1;
        Ok(())
    }

    pub(super) fn apply_approval_final(&mut self, event: &DomainEvent) -> Result<()> {
        let request_event_id = event
            .payload
            .get("request_event_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                Error::Domain(format!(
                    "{} payload.request_event_id is required",
                    event.event_type
                ))
            })?;
        self.clear_approval_interaction(&event.session_id, Some(request_event_id))
    }

    pub(super) fn clear_approval_interaction(
        &mut self,
        session_id: &str,
        expected_request_event_id: Option<&str>,
    ) -> Result<()> {
        let Some(session) = self.sessions.get_mut(session_id) else {
            return Ok(());
        };
        let Some(metadata) = session.metadata.as_object_mut() else {
            return Ok(());
        };
        let matches = metadata
            .get("interaction")
            .and_then(Value::as_object)
            .is_some_and(|interaction| {
                interaction.get("type").and_then(Value::as_str) == Some("approval")
                    && expected_request_event_id.is_none_or(|expected| {
                        interaction.get("request_event_id").and_then(Value::as_str)
                            == Some(expected)
                    })
            });
        if matches {
            metadata.remove("interaction");
            session.state_version += 1;
        }
        Ok(())
    }
}

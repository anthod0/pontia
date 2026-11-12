use serde_json::{Value, json};

use super::{ProjectionState, SessionProjection};
use crate::domain::{DomainEvent, EventSource, EventType};

/// Whether a `session.title_updated` event changes the session title.
///
/// Client titles never replace a manual name, never clear a title, and are
/// dropped when they repeat the current title. Sessions created before
/// `title_source` existed keep any title as a manual name.
pub fn title_update_applies(session: &SessionProjection, event: &DomainEvent) -> bool {
    if !is_client_source(event.source) {
        return true;
    }
    let Some(title) = event.payload["title"].as_str() else {
        return false;
    };
    let manual = match session.metadata["title_source"].as_str() {
        Some(source) => source == "manual",
        None => session.title.is_some(),
    };
    !manual && session.title.as_deref() != Some(title)
}

fn is_client_source(source: EventSource) -> bool {
    matches!(source, EventSource::AgentClient | EventSource::AgentAdapter)
}

impl ProjectionState {
    pub(super) fn apply_title(&mut self, event: &DomainEvent) {
        let Some(session) = self.sessions.get_mut(&event.session_id) else {
            return;
        };
        let title = event.payload["title"]
            .as_str()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned);
        if event.event_type == EventType::SessionCreated {
            session.title = title;
            return;
        }
        if !title_update_applies(session, event) {
            return;
        }
        if !session.metadata.is_object() {
            session.metadata = json!({});
        }
        let source = match (is_client_source(event.source), &title) {
            (true, _) => Some("client"),
            (false, Some(_)) => Some("manual"),
            (false, None) => None,
        };
        let metadata = session
            .metadata
            .as_object_mut()
            .expect("metadata is an object");
        match source {
            Some(source) => metadata.insert("title_source".into(), Value::from(source)),
            None => metadata.remove("title_source"),
        };
        session.title = title;
    }
}

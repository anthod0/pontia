use super::ProjectionState;
use crate::domain::{DomainEvent, EventSource, EventType};

#[derive(Debug, Default, Clone)]
pub(super) struct SessionTitle {
    manual: Option<String>,
    client: Option<String>,
    input: Option<String>,
}

impl SessionTitle {
    fn apply(&mut self, event: &DomainEvent) {
        let title = event.payload["title"]
            .as_str()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned);
        if event.event_type == EventType::SessionCreated
            && event.payload["metadata"]["title_source"] == "input"
        {
            self.input = title;
        } else if event.event_type == EventType::SessionCreated
            || !matches!(
                event.source,
                EventSource::AgentClient | EventSource::AgentAdapter
            )
        {
            self.manual = title;
        } else {
            self.client = title;
        }
    }
}

impl ProjectionState {
    pub fn restore_title_history(&mut self, events: &[DomainEvent]) {
        for event in events {
            self.session_titles
                .entry(event.session_id.clone())
                .or_default()
                .apply(event);
        }
    }

    pub(super) fn apply_title(&mut self, event: &DomainEvent) {
        let title = self
            .session_titles
            .entry(event.session_id.clone())
            .or_default();
        let had_client_title = title.client.is_some();
        title.apply(event);
        // An unnamed client snapshot must preserve a title derived from the input.
        if event.event_type == EventType::SessionTitleUpdated
            && matches!(
                event.source,
                EventSource::AgentClient | EventSource::AgentAdapter
            )
            && !had_client_title
            && title.client.is_none()
            && title.manual.is_none()
        {
            return;
        }
        if let Some(session) = self.sessions.get_mut(&event.session_id) {
            session.title = title
                .manual
                .clone()
                .or_else(|| title.client.clone())
                .or_else(|| title.input.clone());
        }
    }
}

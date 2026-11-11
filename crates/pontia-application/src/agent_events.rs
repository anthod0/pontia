use pontia_core::domain::DomainEvent;

const AGENT_EVENT_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone)]
pub struct AgentEventBroker {
    sender: tokio::sync::broadcast::Sender<DomainEvent>,
    history: tokio::sync::broadcast::Sender<String>,
}

impl AgentEventBroker {
    pub fn history_changed(&self, session: &str) {
        let _ = self.history.send(session.into());
    }
    pub fn subscribe_history(&self) -> tokio::sync::broadcast::Receiver<String> {
        self.history.subscribe()
    }

    pub(crate) fn publish(&self, event: DomainEvent) {
        let _ = self.sender.send(event);
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}

impl Default for AgentEventBroker {
    fn default() -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(AGENT_EVENT_CHANNEL_CAPACITY);
        let (history, _) = tokio::sync::broadcast::channel(AGENT_EVENT_CHANNEL_CAPACITY);
        Self { sender, history }
    }
}

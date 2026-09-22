use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use pontia_core::domain::DomainEvent;

const SESSION_MESSAGE_UPDATED_DEBOUNCE_MS: u64 = 100;

#[derive(Clone)]
pub struct VolatileEventBroker {
    sender: tokio::sync::broadcast::Sender<DomainEvent>,
    debounced_session_message_updates: Arc<Mutex<HashMap<String, DebouncedVolatileEvent>>>,
    next_debounce_generation: Arc<AtomicU64>,
}

struct DebouncedVolatileEvent {
    generation: u64,
    task: tokio::task::JoinHandle<()>,
}

impl VolatileEventBroker {
    pub fn publish(&self, event: DomainEvent) {
        let _ = self.sender.send(event);
    }

    pub fn publish_debounced_session_message_updated(&self, event: DomainEvent) {
        let session_id = event.session_id.clone();
        let generation = self
            .next_debounce_generation
            .fetch_add(1, Ordering::Relaxed);
        let sender = self.sender.clone();
        let pending_updates = self.debounced_session_message_updates.clone();
        let task_session_id = session_id.clone();
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(SESSION_MESSAGE_UPDATED_DEBOUNCE_MS)).await;
            let _ = sender.send(event);
            let mut pending = pending_updates
                .lock()
                .expect("volatile event debounce lock poisoned");
            if pending
                .get(&task_session_id)
                .is_some_and(|pending| pending.generation == generation)
            {
                pending.remove(&task_session_id);
            }
        });

        let mut pending = self
            .debounced_session_message_updates
            .lock()
            .expect("volatile event debounce lock poisoned");
        if let Some(previous) =
            pending.insert(session_id, DebouncedVolatileEvent { generation, task })
        {
            previous.task.abort();
        }
    }

    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DomainEvent> {
        self.sender.subscribe()
    }
}

impl Default for VolatileEventBroker {
    fn default() -> Self {
        let (sender, _) = tokio::sync::broadcast::channel(1024);
        Self {
            sender,
            debounced_session_message_updates: Arc::new(Mutex::new(HashMap::new())),
            next_debounce_generation: Arc::new(AtomicU64::new(1)),
        }
    }
}

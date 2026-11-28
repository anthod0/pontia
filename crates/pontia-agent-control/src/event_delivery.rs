use pontia_core::{Error, Result, domain::DomainEvent};
use pontia_storage_sqlite::repositories::event_subscriptions::EventSubscriptionRepository;
pub use pontia_storage_sqlite::repositories::event_subscriptions::{EventScope, PositionedEvent};
use sqlx::SqlitePool;
use std::{
    collections::HashSet,
    future::Future,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;

use crate::AgentEventBroker;
mod durable;
mod tail;
pub use durable::{DurableSubscription, SubscriptionConfig};
pub use tail::EventTail;

const WINDOW: std::num::NonZeroU32 = std::num::NonZeroU32::new(4096).unwrap();

#[derive(Clone)]
pub struct EventDeliveryService {
    store: EventSubscriptionRepository,
    broker: AgentEventBroker,
    subscribers: Arc<Mutex<HashSet<String>>>,
}

impl EventDeliveryService {
    pub(crate) fn new(db: SqlitePool, broker: AgentEventBroker) -> Self {
        Self {
            store: EventSubscriptionRepository::new(db),
            broker,
            subscribers: Default::default(),
        }
    }

    /// Registers one handler for this ID over the lifetime of this service.
    /// The handler must tolerate replay if its work commits before the cursor does.
    pub async fn register_durable<F, Fut>(
        &self,
        config: SubscriptionConfig,
        handler: F,
    ) -> Result<DurableSubscription>
    where
        F: Fn(PositionedEvent) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.store.register(&config.subscriber_id).await?;
        if !self
            .subscribers
            .lock()
            .expect("subscriber registry poisoned")
            .insert(config.subscriber_id.clone())
        {
            return Err(Error::StateConflict(format!(
                "Subscriber {} is already registered",
                config.subscriber_id
            )));
        }
        Ok(DurableSubscription::new(
            self.clone(),
            config,
            move |event| Box::pin(handler(event)),
        ))
    }

    pub fn tail(&self, scope: EventScope, after_position: i64, stream_once: bool) -> EventTail {
        EventTail::new(self, scope, after_position, stream_once)
    }

    pub async fn current_head_position(&self) -> Result<i64> {
        self.store.current_head_position().await
    }

    pub async fn resolve_event_position(
        &self,
        scope: &EventScope,
        event_id: &str,
    ) -> Result<Option<i64>> {
        self.store.resolve_event_position(scope, event_id).await
    }

    pub async fn has_session_event_after(
        &self,
        session_id: &str,
        after_position: i64,
        event_types: &[String],
    ) -> Result<bool> {
        self.store
            .has_session_event_after(session_id, after_position, event_types)
            .await
    }

    pub fn subscribe_live(&self) -> broadcast::Receiver<DomainEvent> {
        self.broker.subscribe()
    }
    pub fn subscribe_history(&self) -> broadcast::Receiver<String> {
        self.broker.subscribe_history()
    }
}

#[cfg(test)]
mod tests;

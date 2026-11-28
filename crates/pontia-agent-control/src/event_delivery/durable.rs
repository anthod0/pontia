use super::*;
use std::{num::NonZeroU32, pin::Pin, time::Duration};
use tokio::{
    sync::{Mutex, watch},
    time::{Instant, sleep_until},
};

#[derive(Clone)]
pub struct SubscriptionConfig {
    pub subscriber_id: String,
    pub scope: EventScope,
    pub event_types: Vec<String>,
    pub scan_window: NonZeroU32,
}

impl SubscriptionConfig {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            subscriber_id: id.into(),
            scope: EventScope::All,
            event_types: vec![],
            scan_window: WINDOW,
        }
    }
}

type HandlerFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;
const INITIAL_DELAY: Duration = Duration::from_millis(250);
const MAX_DELAY: Duration = Duration::from_secs(30);

struct Retry {
    deadline: Option<Instant>,
    delay: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Self {
            deadline: None,
            delay: INITIAL_DELAY,
        }
    }
}

pub struct DurableSubscription {
    service: EventDeliveryService,
    config: SubscriptionConfig,
    handler: Box<dyn Fn(PositionedEvent) -> HandlerFuture + Send + Sync>,
    delivery: Mutex<Retry>,
}

impl DurableSubscription {
    pub(super) fn new(
        service: EventDeliveryService,
        config: SubscriptionConfig,
        handler: impl Fn(PositionedEvent) -> HandlerFuture + Send + Sync + 'static,
    ) -> Self {
        Self {
            service,
            config,
            handler: Box::new(handler),
            delivery: Default::default(),
        }
    }

    /// Drain through the head observed after acquiring the delivery lock. A failed delivery is returned to the caller.
    pub async fn catch_up(&self) -> Result<()> {
        let (_sender, mut shutdown) = watch::channel(false);
        self.drain(&mut shutdown).await
    }

    pub async fn run(&self, mut shutdown: watch::Receiver<bool>) -> Result<()> {
        let mut commits = self.service.broker.head_position.subscribe();
        loop {
            if is_stopped(&shutdown) {
                return Ok(());
            }
            // Observe pending notifications before reading the authoritative head.
            commits.borrow_and_update();
            if let Err(error) = self.drain(&mut shutdown).await {
                tracing::warn!(subscriber=%self.config.subscriber_id, %error, "Event delivery failed");
                continue;
            }
            tokio::select! {
                _ = stopped(&mut shutdown) => return Ok(()),
                result = commits.changed() => if result.is_err() { return Ok(()); },
            }
        }
    }

    async fn drain(&self, shutdown: &mut watch::Receiver<bool>) -> Result<()> {
        let mut retry = tokio::select! {
            biased;
            _ = stopped(shutdown) => return Ok(()),
            lock = self.delivery.lock() => lock,
        };
        if let Some(deadline) = retry.deadline {
            tokio::select! {
                _ = stopped(shutdown) => return Ok(()),
                _ = sleep_until(deadline) => {}
            }
        }
        let result = self.deliver(shutdown, &mut retry).await;
        if result.is_err() {
            retry.deadline = Some(Instant::now() + retry.delay);
            retry.delay = (retry.delay * 2).min(MAX_DELAY);
        }
        result
    }

    async fn deliver(&self, shutdown: &watch::Receiver<bool>, retry: &mut Retry) -> Result<()> {
        let store = &self.service.store;
        let config = &self.config;
        let head = store.current_head_position().await?;
        let mut cursor = store.cursor(&config.subscriber_id).await?;
        while cursor < head && !is_stopped(shutdown) {
            let window = store
                .scan(
                    &config.scope,
                    &config.event_types,
                    cursor,
                    head,
                    config.scan_window,
                )
                .await?;
            for event in window.events {
                if is_stopped(shutdown) {
                    return Ok(());
                }
                let position = event.position;
                (self.handler)(event).await?;
                store.advance(&config.subscriber_id, position).await?;
                cursor = position;
                *retry = Retry::default();
            }
            if cursor < window.end_position {
                cursor = window.end_position;
                store.advance(&config.subscriber_id, cursor).await?;
            }
        }
        Ok(())
    }
}

async fn stopped(shutdown: &mut watch::Receiver<bool>) {
    let _ = shutdown.wait_for(|stop| *stop).await;
}

fn is_stopped(shutdown: &watch::Receiver<bool>) -> bool {
    *shutdown.borrow() || shutdown.has_changed().is_err()
}

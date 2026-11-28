use super::HelperService;
use crate::conversation::ConversationService;
use pontia_core::Result;
use sqlx::SqlitePool;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{Notify, watch},
    time::{Instant, timeout_at},
};

const FIRST_RETRY: Duration = Duration::from_secs(2);
const MAX_RETRY: Duration = Duration::from_secs(300);

/// Per-Bot retry schedule for failed notification deliveries.
#[derive(Default)]
struct Backoff(HashMap<String, (u32, Instant)>);

impl Backoff {
    fn retry_at(&self, bot_id: &str, now: Instant) -> Option<Instant> {
        self.0.get(bot_id).map(|(_, at)| *at).filter(|at| *at > now)
    }

    fn failed(&mut self, bot_id: &str, now: Instant) -> (u32, Duration) {
        let failures = self.0.get(bot_id).map_or(0, |(failures, _)| *failures) + 1;
        let delay = retry_delay(failures);
        self.0.insert(bot_id.into(), (failures, now + delay));
        (failures, delay)
    }

    fn delivered(&mut self, bot_id: &str) {
        self.0.remove(bot_id);
    }
}

fn retry_delay(failures: u32) -> Duration {
    FIRST_RETRY
        .saturating_mul(2u32.saturating_pow(failures.saturating_sub(1)))
        .min(MAX_RETRY)
}

#[derive(Clone)]
pub(super) struct NotificationQueue {
    pub(super) pool: SqlitePool,
    wake: Arc<Notify>,
}

impl NotificationQueue {
    pub(super) fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            wake: Arc::default(),
        }
    }

    pub(super) async fn enqueue(
        &self,
        id: &str,
        bot_id: &str,
        source: &str,
        content: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO bot_notifications(id, bot_id, source_session_id, content) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO NOTHING")
            .bind(id).bind(bot_id).bind(source).bind(content).execute(&self.pool).await?;
        self.wake.notify_one();
        Ok(())
    }
}

impl HelperService {
    pub fn notification_delivery(&self) -> NotificationDelivery {
        NotificationDelivery {
            queue: self.notifications.clone(),
            conversation: self.conversation.clone(),
            backoff: Backoff::default(),
        }
    }
}

pub struct NotificationDelivery {
    queue: NotificationQueue,
    conversation: ConversationService,
    backoff: Backoff,
}

impl NotificationDelivery {
    pub async fn run(mut self, mut shutdown: watch::Receiver<bool>) {
        while !*shutdown.borrow() && shutdown.has_changed().is_ok() {
            let retry = match self.deliver_pending().await {
                Ok(next) => next,
                Err(error) => {
                    tracing::warn!(%error, "Reading or updating pending Bot notifications failed");
                    Some(Instant::now() + FIRST_RETRY)
                }
            };
            tokio::select! {
                _ = shutdown.wait_for(|stop| *stop) => break,
                _ = async {
                    let notified = self.queue.wake.notified();
                    match retry {
                        Some(at) => { let _ = timeout_at(at, notified).await; }
                        None => notified.await,
                    }
                } => (),
            }
        }
    }

    pub async fn deliver_pending(&mut self) -> Result<Option<Instant>> {
        let notifications: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT id, bot_id, source_session_id, content FROM bot_notifications WHERE delivered = 0 ORDER BY created_at, id"
        ).fetch_all(&self.queue.pool).await?;
        let mut next_retry = None;
        let scan_time = Instant::now();
        for (id, bot_id, source, content) in notifications {
            if let Some(at) = self.backoff.retry_at(&bot_id, scan_time) {
                next_retry = Some(at.min(next_retry.unwrap_or(at)));
                continue;
            }
            let source = source
                .map(|session| format!("\nHelper Session: {session}"))
                .unwrap_or_default();
            match self
                .conversation
                .deliver_input(
                    &bot_id,
                    &format!("Source: System Notification{source}\n{content}"),
                )
                .await
            {
                Ok(_) => {
                    self.backoff.delivered(&bot_id);
                    sqlx::query("UPDATE bot_notifications SET delivered = 1 WHERE id = ?")
                        .bind(&id)
                        .execute(&self.queue.pool)
                        .await?;
                }
                Err(error) => {
                    let now = Instant::now();
                    let (failures, retry_in) = self.backoff.failed(&bot_id, now);
                    let at = now + retry_in;
                    next_retry = Some(at.min(next_retry.unwrap_or(at)));
                    tracing::warn!(%bot_id, notification_id = %id, %error, failures, ?retry_in, "Bot notification delivery failed and remains pending");
                }
            }
        }
        Ok(next_retry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_delay_doubles_up_to_the_cap() {
        let delays: Vec<u64> = (1..=9).map(|n| retry_delay(n).as_secs()).collect();
        assert_eq!(delays, [2, 4, 8, 16, 32, 64, 128, 256, 300]);
        assert_eq!(retry_delay(u32::MAX), MAX_RETRY);
    }

    #[test]
    fn backoff_holds_a_bot_until_its_retry_time_and_resets_on_delivery() {
        let mut backoff = Backoff::default();
        let start = Instant::now();
        assert!(backoff.retry_at("bot", start).is_none());
        assert_eq!(backoff.failed("bot", start), (1, FIRST_RETRY));
        assert_eq!(
            backoff.retry_at("bot", start + FIRST_RETRY / 2),
            Some(start + FIRST_RETRY)
        );
        assert!(backoff.retry_at("other", start).is_none());
        let second = start + FIRST_RETRY;
        assert!(backoff.retry_at("bot", second).is_none());
        assert_eq!(backoff.failed("bot", second), (2, FIRST_RETRY * 2));
        assert_eq!(
            backoff.retry_at("bot", second + FIRST_RETRY),
            Some(second + FIRST_RETRY * 2)
        );
        backoff.delivered("bot");
        assert!(backoff.retry_at("bot", second).is_none());
        assert_eq!(backoff.failed("bot", second), (1, FIRST_RETRY));
    }
}

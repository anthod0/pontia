use super::{HelperService, associate, helper_bot};
use pontia_agent_control::SessionOwnership;
use pontia_core::{Result, domain::EventType};
use serde_json::Value;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tokio::sync::{broadcast::error::RecvError, watch};

const FIRST_RETRY: Duration = Duration::from_secs(2);
const MAX_RETRY: Duration = Duration::from_secs(300);

/// Per-Bot retry schedule for failed notification deliveries.
#[derive(Default)]
pub(crate) struct Backoff(HashMap<String, (u32, Instant)>);

impl Backoff {
    fn ready(&self, bot_id: &str, now: Instant) -> bool {
        self.0
            .get(bot_id)
            .is_none_or(|(_, retry_at)| now >= *retry_at)
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

impl HelperService {
    pub(crate) async fn enqueue(
        &self,
        id: &str,
        bot_id: &str,
        source: Option<&str>,
        content: &str,
    ) -> Result<()> {
        sqlx::query("INSERT INTO bot_notifications(id, bot_id, source_session_id, content) VALUES (?, ?, ?, ?) ON CONFLICT(id) DO NOTHING")
            .bind(id).bind(bot_id).bind(source).bind(content).execute(&self.control.db()).await?;
        Ok(())
    }

    /// Durable events are authoritative; the broadcast only shortens polling latency.
    /// Only events that reconciliation turns into notifications wake it early, so the turn and
    /// usage events of a failed delivery cannot immediately trigger another attempt.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut events = self.control.agent_events().subscribe();
        let mut tick = tokio::time::interval(Duration::from_secs(2));
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tick.tick() => (),
                event = events.recv() => match event {
                    Err(RecvError::Closed) => break,
                    Ok(event) if !matches!(
                        event.event_type,
                        EventType::TurnFailed | EventType::SessionError | EventType::SessionExited
                    ) => continue,
                    _ => (),
                }
            }
            if let Err(error) = self.reconcile().await {
                tracing::warn!(%error, "Bot helper notification reconciliation failed");
            }
        }
    }

    pub async fn reconcile(&self) -> Result<()> {
        let _guard = self.reconciliation.lock().await;
        let pool = self.control.db();
        let pending: Vec<String> =
            sqlx::query_scalar("SELECT reference FROM bot_helpers WHERE session_id IS NULL")
                .fetch_all(&pool)
                .await?;
        for reference in pending {
            if let Some(session) = self
                .control
                .queries()
                .find_session_by_ownership(&SessionOwnership {
                    application_id: "bot".into(),
                    reference: reference.clone(),
                })
                .await?
            {
                associate(&pool, &reference, &session).await?;
            }
        }
        loop {
            let position: i64 =
                sqlx::query_scalar("SELECT position FROM bot_event_cursor WHERE id = 1")
                    .fetch_one(&pool)
                    .await?;
            let page = self
                .control
                .queries()
                .domain_events()
                .read_after(position, Some("bot"), 100)
                .await?;
            for event in page.events {
                if !matches!(
                    event.event_type.as_str(),
                    "turn.failed" | "session.error" | "session.exited"
                ) {
                    continue;
                }
                // Creation can finish between the pending lookup and this event page.
                let bot_id = match helper_bot(&pool, &event.session_id).await? {
                    Some(bot) => Some(bot),
                    None => match self
                        .control
                        .queries()
                        .session_ownership(&event.session_id)
                        .await?
                    {
                        Some(ownership) => {
                            let bot: Option<String> = sqlx::query_scalar(
                                "SELECT bot_id FROM bot_helpers WHERE reference = ?",
                            )
                            .bind(&ownership.reference)
                            .fetch_optional(&pool)
                            .await?;
                            if bot.is_some() {
                                associate(&pool, &ownership.reference, &event.session_id).await?;
                            }
                            bot
                        }
                        None => None,
                    },
                };
                if let Some(bot_id) = bot_id {
                    let payload: Value = serde_json::from_str(&event.payload)?;
                    let detail = ["reason", "failure", "error"]
                        .into_iter()
                        .filter_map(|key| {
                            payload
                                .get(key)
                                .filter(|value| !value.is_null())
                                .map(|value| format!("\nKnown {key}: {value}"))
                        })
                        .collect::<String>();
                    let qualification = if event.event_type == "session.exited" {
                        "\nSession exit does not establish task failure or termination of native execution or a shared client process."
                    } else {
                        ""
                    };
                    self.enqueue(
                        &format!("event:{}", event.event_id),
                        &bot_id,
                        Some(&event.session_id),
                        &format!(
                            "Observed {} at {} (event {}).{detail}{qualification}",
                            event.event_type, event.occurred_at, event.event_id
                        ),
                    )
                    .await?;
                }
            }
            // Notification IDs make replay safe if the process stops before advancing the cursor.
            sqlx::query("UPDATE bot_event_cursor SET position = ? WHERE id = 1")
                .bind(page.next_position)
                .execute(&pool)
                .await?;
            if page.next_position >= page.head_position {
                break;
            }
        }
        let notifications: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
            "SELECT id, bot_id, source_session_id, content FROM bot_notifications WHERE delivered = 0 ORDER BY created_at, id"
        ).fetch_all(&pool).await?;
        let mut unavailable = std::collections::HashSet::new();
        for (id, bot_id, source, content) in notifications {
            if unavailable.contains(&bot_id)
                || !self
                    .delivery_backoff
                    .lock()
                    .unwrap()
                    .ready(&bot_id, Instant::now())
            {
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
                    self.delivery_backoff.lock().unwrap().delivered(&bot_id);
                    sqlx::query("UPDATE bot_notifications SET delivered = 1 WHERE id = ?")
                        .bind(&id)
                        .execute(&pool)
                        .await?;
                }
                Err(error) => {
                    let (failures, retry_in) = self
                        .delivery_backoff
                        .lock()
                        .unwrap()
                        .failed(&bot_id, Instant::now());
                    tracing::warn!(%bot_id, notification_id = %id, %error, failures, ?retry_in, "Bot notification delivery failed and remains pending");
                    unavailable.insert(bot_id);
                }
            }
        }
        Ok(())
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
        assert!(backoff.ready("bot", start));
        assert_eq!(backoff.failed("bot", start), (1, FIRST_RETRY));
        assert!(!backoff.ready("bot", start + FIRST_RETRY / 2));
        assert!(backoff.ready("other", start));
        let second = start + FIRST_RETRY;
        assert!(backoff.ready("bot", second));
        assert_eq!(backoff.failed("bot", second), (2, FIRST_RETRY * 2));
        assert!(!backoff.ready("bot", second + FIRST_RETRY));
        backoff.delivered("bot");
        assert!(backoff.ready("bot", second));
        assert_eq!(backoff.failed("bot", second), (1, FIRST_RETRY));
    }
}

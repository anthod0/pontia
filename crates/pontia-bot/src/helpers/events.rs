use super::{HelperService, associate, notifications::NotificationQueue};
use pontia_agent_control::{
    DurableSubscription, EventScope, PositionedEvent, SessionOwnership, SubscriptionConfig,
};
use pontia_core::{Result, domain::EventType};
use serde_json::Value;

impl HelperService {
    pub async fn register_events(&self) -> Result<DurableSubscription> {
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

        let handler = HelperEventHandler {
            queue: self.notifications.clone(),
        };
        let mut config = SubscriptionConfig::new("bot.helper_notifications");
        config.scope = EventScope::Application("bot".into());
        config.event_types = [
            EventType::TurnFailed,
            EventType::SessionError,
            EventType::SessionExited,
        ]
        .map(|kind| kind.to_string())
        .into();
        self.control
            .events()
            .register_durable(config, move |event| {
                let handler = handler.clone();
                async move { handler.handle(event).await }
            })
            .await
    }
}

#[derive(Clone)]
struct HelperEventHandler {
    queue: NotificationQueue,
}

impl HelperEventHandler {
    async fn handle(&self, event: PositionedEvent) -> Result<()> {
        // A launch failure can arrive before create() associates the Helper Session.
        let bot_id: Option<String> = sqlx::query_scalar(
            "UPDATE bot_helpers SET session_id = ? WHERE reference = ? RETURNING bot_id",
        )
        .bind(&event.session_id)
        .bind(&event.application_ref)
        .fetch_optional(&self.queue.pool)
        .await?;
        let Some(bot_id) = bot_id else { return Ok(()) };
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
        self.queue
            .enqueue(
                &format!("event:{}", event.event_id),
                &bot_id,
                &event.session_id,
                &format!(
                    "Observed {} at {} (event {}).{detail}{qualification}",
                    event.event_type, event.occurred_at, event.event_id
                ),
            )
            .await
    }
}

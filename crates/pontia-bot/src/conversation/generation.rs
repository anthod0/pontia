use super::{Activity, ConversationService, ConversationUpdate, Notice};
use pontia_core::{Error, Result};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::watch;

pub(super) enum SwitchFailure {
    Failed(String),
    Unknown(String),
}

impl SwitchFailure {
    pub(super) fn error(&self) -> Error {
        match self {
            Self::Failed(reason) => Error::Domain(reason.clone()),
            Self::Unknown(reason) => Error::ControlUnknown(reason.clone()),
        }
    }
}

#[derive(Clone, Copy)]
pub struct GenerationLimits {
    token_limit: u64,
    window_percent: u8,
}

impl Default for GenerationLimits {
    fn default() -> Self {
        Self {
            token_limit: 258_000,
            window_percent: 80,
        }
    }
}

impl GenerationLimits {
    pub fn new(token_limit: u64, window_percent: u8) -> Result<Self> {
        if token_limit == 0 || !(1..=100).contains(&window_percent) {
            return Err(Error::Domain(
                "Bot token limit must be positive and window percent must be between 1 and 100"
                    .into(),
            ));
        }
        Ok(Self {
            token_limit,
            window_percent,
        })
    }

    fn exceeded(self, used: u64, window: Option<u64>) -> bool {
        let limit = window
            .filter(|window| *window > 0)
            .map(|window| (u128::from(window) * u128::from(self.window_percent) / 100) as u64)
            .unwrap_or(self.token_limit)
            .min(self.token_limit);
        used > limit
    }
}

impl ConversationService {
    pub fn with_generation_limits(mut self, limits: GenerationLimits) -> Self {
        self.generation_limits = limits;
        self
    }

    pub async fn run_generations(self, mut shutdown: watch::Receiver<bool>) {
        let mut events = self.control.agent_events().subscribe();
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tick.tick() => (),
                event = events.recv() => {
                    if matches!(event, Err(tokio::sync::broadcast::error::RecvError::Closed)) { break; }
                }
            }
            if let Err(error) = self.reconcile_generations(OffsetDateTime::now_utc()).await {
                tracing::warn!(%error, "Bot generation reconciliation failed");
            }
        }
    }

    pub async fn reconcile_generations(&self, now: OffsetDateTime) -> Result<()> {
        for bot in self.records.list().await? {
            let activity = self.activity(&bot.bot_id);
            // Input delivery owns the same gate; skip Bots already handling an operation.
            let Ok(_delivery) = activity.delivery.try_lock() else {
                continue;
            };
            // rotate exposes failures through the conversation notice and log.
            let _ = self.rotate(&bot.bot_id, &activity, now).await;
            self.retire_obsolete(&bot.bot_id, &activity).await?;
        }
        Ok(())
    }

    pub(super) async fn rotate(
        &self,
        bot_id: &str,
        activity: &Activity,
        now: OffsetDateTime,
    ) -> Result<()> {
        let result = self.rotate_inner(bot_id, activity, now).await;
        if let Err(error) = &result {
            let mut failure = activity.switch_failure.lock().unwrap();
            failure.0 += 1;
            failure.1 = Some(match error {
                Error::ControlUnknown(reason) => SwitchFailure::Unknown(reason.clone()),
                error => SwitchFailure::Failed(error.to_string()),
            });
            self.generation_error(bot_id, activity, error);
        }
        result
    }

    pub(super) fn generation_error(&self, bot_id: &str, activity: &Activity, error: &Error) {
        tracing::warn!(%bot_id, %error, "Bot generation operation failed");
        let mut notice = activity.generation_notice.lock().unwrap();
        if notice.is_none() {
            *notice = Some(Notice {
                id: uuid::Uuid::now_v7().to_string(),
                kind: "bot.generation_failed".into(),
                created_at: pontia_core::time::utc_now()
                    .format(&Rfc3339)
                    .expect("UTC timestamp"),
            });
            activity.notify(ConversationUpdate::Changed);
        }
    }

    async fn rotate_inner(
        &self,
        bot_id: &str,
        activity: &Activity,
        now: OffsetDateTime,
    ) -> Result<()> {
        let bot = self.records.get(bot_id).await?;
        let Some(session) = &bot.current_session_id else {
            return Ok(());
        };
        let snapshot = self
            .control
            .queries()
            .session_snapshot(session)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session} not found")))?;
        if snapshot.state == "exited" {
            self.records.set_current_session(bot_id, None).await?;
            activity.notify(ConversationUpdate::Changed);
            return Ok(());
        }
        if snapshot.state != "idle"
            || snapshot.has_unsettled_input
            || snapshot
                .turns
                .iter()
                .any(|turn| matches!(turn.state.as_str(), "queued" | "running"))
        {
            return Ok(());
        }

        // Read existing facts instead of persisting a second lifecycle projection.
        // Earlier high usage still requests handoff after native compaction reduces it.
        let events = self.control.queries().list_session_events(session).await?;
        let mut used = None;
        let mut window = None;
        let mut upper = false;
        let mut idle_since = None;
        for event in events {
            if event.event_type == "session.context_usage_updated" {
                let usage = &event.payload["context_usage"];
                if let Some(value) = usage.get("used_tokens") {
                    used = value.as_u64();
                }
                if let Some(value) = usage.get("max_tokens") {
                    window = value.as_u64();
                }
                upper |= used.is_some_and(|used| self.generation_limits.exceeded(used, window));
            }
            let idle_boundary = match event.event_type.as_str() {
                "turn.completed" | "turn.failed" => event.turn_id == snapshot.current_turn_id,
                "session.ready" => snapshot.current_turn_id.is_none() && idle_since.is_none(),
                _ => false,
            };
            if idle_boundary && idle_since.is_none() {
                idle_since = Some(OffsetDateTime::parse(&event.time, &Rfc3339).map_err(
                    |error| Error::Domain(format!("Invalid Session event time: {error}")),
                )?);
            }
        }
        let lower = used.is_some_and(|used| used > 75_000)
            && idle_since.is_some_and(|since| now - since >= time::Duration::minutes(5));
        if !upper && !lower {
            return Ok(());
        }
        let next = if upper {
            Some(self.start_session(&bot, activity).await?)
        } else {
            None
        };
        if self
            .control
            .session_commands()
            .exit_if_idle(session)
            .await?
        {
            self.records
                .set_current_session(bot_id, next.as_deref())
                .await?;
            activity.generation_notice.lock().unwrap().take();
            activity.notify(ConversationUpdate::Changed);
        }
        Ok(())
    }

    async fn retire_obsolete(&self, bot_id: &str, activity: &Activity) -> Result<()> {
        let current = self.records.get(bot_id).await?.current_session_id;
        for session in self.control.queries().list_owned_sessions("bot").await? {
            if current.as_deref() == Some(&session) {
                continue;
            }
            let ownership = self
                .control
                .queries()
                .session_ownership(&session)
                .await?
                .expect("owned Session has ownership");
            if !ownership.reference.starts_with(&format!("{bot_id}:")) {
                continue;
            }
            let snapshot = self
                .control
                .queries()
                .session_snapshot(&session)
                .await?
                .expect("owned Session exists");
            let result = if matches!(snapshot.state.as_str(), "error" | "interrupted") {
                self.control
                    .session_commands()
                    .terminate_session(&session)
                    .await
                    .map(|_| ())
            } else {
                self.control
                    .session_commands()
                    .exit_if_idle(&session)
                    .await
                    .map(|_| ())
            };
            if let Err(error) = result {
                self.generation_error(bot_id, activity, &error);
            }
        }
        Ok(())
    }
}

use super::{Activity, ConversationService, ConversationUpdate, Notice};
use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventType},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::watch;

const RECONCILE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);
const IDLE_TOKEN_LIMIT: u64 = 75_000;
const IDLE_DURATION: time::Duration = time::Duration::minutes(5);

fn should_reconcile(event: &DomainEvent) -> bool {
    matches!(
        event.event_type,
        EventType::SessionReady
            | EventType::SessionExited
            | EventType::SessionError
            | EventType::SessionCompactionCompleted
            | EventType::SessionContextUsageUpdated
            | EventType::TurnStarted
            | EventType::TurnCompleted
            | EventType::TurnFailed
            | EventType::TurnInterrupted
    )
}

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

    pub async fn reload(&self, bot_id: &str) -> Result<()> {
        let activity = self.activity(bot_id);
        let _delivery = activity.delivery.lock().await;
        let Some(session) = self.records.get(bot_id).await?.current_session_id else {
            return Ok(());
        };
        if !self
            .switch_if_idle(bot_id, &activity, &session, None)
            .await
            .inspect_err(|error| self.record_switch_failure(bot_id, &activity, error))?
        {
            return Err(Error::StateConflict(
                "Bot Session is busy. Wait for it to become idle before reloading.".into(),
            ));
        }
        Ok(())
    }

    pub async fn run_generations(self, mut shutdown: watch::Receiver<bool>) {
        let mut events = self.control.agent_events().subscribe();
        let mut tick = tokio::time::interval(RECONCILE_INTERVAL);
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = tick.tick() => (),
                event = events.recv() => match event {
                    Ok(event) if !should_reconcile(&event) => continue,
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => (),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
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
        self.rotate_inner(bot_id, activity, now)
            .await
            .inspect_err(|error| self.record_switch_failure(bot_id, activity, error))
    }

    fn record_switch_failure(&self, bot_id: &str, activity: &Activity, error: &Error) {
        let mut failure = activity.switch_failure.lock().unwrap();
        failure.0 += 1;
        failure.1 = Some(match error {
            Error::ControlUnknown(reason) => SwitchFailure::Unknown(reason.clone()),
            error => SwitchFailure::Failed(error.to_string()),
        });
        self.generation_error(bot_id, activity, error);
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
            activity.notify(ConversationUpdate::Reset);
            return Ok(());
        }
        if snapshot.state != "idle"
            || snapshot.has_unsettled_input
            || snapshot
                .turns
                .iter()
                .any(|turn| matches!(turn.state.as_str(), "queued" | "running"))
        {
            self.records.clear_idle_timer(bot_id).await?;
            return Ok(());
        }

        let session_view = self
            .control
            .queries()
            .get_session(session)
            .await?
            .ok_or_else(|| Error::NotFound(format!("Session {session} not found")))?;
        let used = session_view
            .context_usage
            .as_ref()
            .and_then(|usage| usage.used_tokens);
        let window = session_view
            .context_usage
            .as_ref()
            .and_then(|usage| usage.max_tokens);
        let compacted = self
            .control
            .queries()
            .list_session_events(session)
            .await?
            .iter()
            .any(|event| event.event_type == "session.compaction_completed");
        let immediate =
            compacted || used.is_some_and(|used| self.generation_limits.exceeded(used, window));
        let lower = if immediate {
            false
        } else if used.is_some_and(|used| used > IDLE_TOKEN_LIMIT) {
            let timer = self.records.idle_timer(bot_id).await?;
            let current_turn = snapshot.current_turn_id.as_deref();
            match timer {
                Some(timer) if timer.turn_id.as_deref() == current_turn => {
                    let since = OffsetDateTime::parse(&timer.since, &Rfc3339).map_err(|error| {
                        Error::Domain(format!("Invalid Bot idle_since timestamp: {error}"))
                    })?;
                    now - since >= IDLE_DURATION
                }
                _ => {
                    let timestamp = now.format(&Rfc3339).expect("UTC timestamp");
                    self.records
                        .set_idle_timer(bot_id, &timestamp, current_turn)
                        .await?;
                    false
                }
            }
        } else {
            self.records.clear_idle_timer(bot_id).await?;
            false
        };
        if !immediate && !lower {
            return Ok(());
        }
        let next = if immediate {
            Some(self.start_session(&bot, activity).await?)
        } else {
            None
        };
        if !self
            .switch_if_idle(bot_id, activity, session, next.as_deref())
            .await?
        {
            self.records.clear_idle_timer(bot_id).await?;
        }
        Ok(())
    }

    async fn switch_if_idle(
        &self,
        bot_id: &str,
        activity: &Activity,
        session: &str,
        next: Option<&str>,
    ) -> Result<bool> {
        if !self
            .control
            .session_commands()
            .exit_if_idle(session)
            .await?
        {
            return Ok(false);
        }
        self.records.set_current_session(bot_id, next).await?;
        activity.generation_notice.lock().unwrap().take();
        activity.notify(ConversationUpdate::Reset);
        Ok(true)
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

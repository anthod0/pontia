use super::InboxCommandService;
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU8, Ordering},
    },
};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Clone, Default)]
pub(crate) struct InboxScheduler(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    sessions: HashMap<String, Arc<SessionActor>>,
    running: HashMap<String, bool>,
    stopped: bool,
    // AgentControl owns delivery; a weak link prevents the event/scheduler/delivery cycle from leaking.
    delivery: Weak<InboxCommandService>,
    lifecycle: Weak<crate::SessionCommandService>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SessionPhase {
    #[default]
    Idle,
    StartingTurn,
    Busy,
    Interrupting,
}

#[derive(Debug, Default)]
pub(crate) struct SessionActor {
    serial: AsyncMutex<()>,
    phase: AtomicU8,
}

impl SessionActor {
    pub(crate) async fn lock(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.serial.lock().await
    }

    pub(crate) fn set_phase(&self, phase: SessionPhase) {
        self.phase.store(phase as u8, Ordering::Release);
    }

    pub(crate) fn finish_start(&self) {
        let _ = self.phase.compare_exchange(
            SessionPhase::StartingTurn as u8,
            SessionPhase::Busy as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(crate) fn cancel_start(&self) {
        let _ = self.phase.compare_exchange(
            SessionPhase::StartingTurn as u8,
            SessionPhase::Idle as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    pub(crate) fn phase(&self) -> SessionPhase {
        match self.phase.load(Ordering::Acquire) {
            value if value == SessionPhase::Idle as u8 => SessionPhase::Idle,
            value if value == SessionPhase::StartingTurn as u8 => SessionPhase::StartingTurn,
            value if value == SessionPhase::Busy as u8 => SessionPhase::Busy,
            value if value == SessionPhase::Interrupting as u8 => SessionPhase::Interrupting,
            _ => unreachable!("SessionPhase stores only declared discriminants"),
        }
    }
}

impl InboxScheduler {
    pub(crate) fn connect(
        &self,
        delivery: &Arc<InboxCommandService>,
        lifecycle: &Arc<crate::SessionCommandService>,
    ) {
        let mut state = self.0.lock().expect("inbox scheduler lock");
        state.delivery = Arc::downgrade(delivery);
        state.lifecycle = Arc::downgrade(lifecycle);
    }

    pub(crate) fn session(&self, session: &str) -> Arc<SessionActor> {
        self.0
            .lock()
            .expect("inbox scheduler lock")
            .sessions
            .entry(session.into())
            .or_default()
            .clone()
    }

    pub(crate) fn observe_phase(&self, session: &str, phase: SessionPhase) {
        let actor = self.session(session);
        actor.set_phase(phase);
    }

    pub fn wake(&self, session: String) {
        let mut state = self.0.lock().expect("inbox scheduler lock");
        if state.stopped {
            return;
        }
        if let Some(pending) = state.running.get_mut(&session) {
            *pending = true;
            return;
        }
        let Some(inbox) = state.delivery.upgrade() else {
            return;
        };
        let lifecycle = state.lifecycle.upgrade();
        state.running.insert(session.clone(), false);
        let scheduler = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = inbox.drain_inbox(&session).await {
                    tracing::warn!(%session, %error, "Inbox scheduling failed");
                }
                if let Some(lifecycle) = &lifecycle
                    && let Err(error) = lifecycle.auto_exit_if_done(&session).await
                {
                    tracing::warn!(%session, %error, "Automatic Session exit failed");
                }
                let mut state = scheduler.0.lock().expect("inbox scheduler lock");
                if !state.stopped && state.running.get(&session) == Some(&true) {
                    state.running.insert(session.clone(), false);
                } else {
                    state.running.remove(&session);
                    break;
                }
            }
        });
    }

    pub async fn stop(&self) {
        self.0.lock().expect("inbox scheduler lock").stopped = true;
        loop {
            if self
                .0
                .lock()
                .expect("inbox scheduler lock")
                .running
                .is_empty()
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
}

impl InboxCommandService {
    pub fn notify_available(&self, session: &str) {
        self.scheduler.wake(session.into());
    }

    pub async fn recover_deliveries(&self) -> pontia_core::Result<()> {
        sqlx::query("UPDATE inbox_messages SET state='failed',failure_message='Session recovery was interrupted before input delivery; retry explicitly',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE state='resuming'")
            .execute(&self.pool).await?;
        sqlx::query("UPDATE inbox_messages SET state=CASE WHEN turn_id IS NULL THEN 'unknown' ELSE 'dispatched' END,failure_message=CASE WHEN turn_id IS NULL THEN 'Delivery is uncertain after Pontia restarted; input was not retried' ELSE NULL END,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE state='dispatching'")
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn resume_pending(&self) -> pontia_core::Result<()> {
        let sessions: Vec<String> = sqlx::query_scalar(
            "SELECT session_id FROM inbox_messages WHERE state='pending' UNION SELECT session_id FROM sessions WHERE exit_when_done=1 AND state='idle'",
        )
        .fetch_all(&self.pool)
        .await?;
        for session in sessions {
            self.notify_available(&session);
        }
        Ok(())
    }

    pub async fn stop_scheduling(&self) {
        self.scheduler.stop().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn committed_phase_updates_are_not_dropped_while_the_actor_is_locked() {
        let scheduler = InboxScheduler::default();
        let actor = scheduler.session("session");
        let _guard = actor.lock().await;
        actor.set_phase(SessionPhase::StartingTurn);

        scheduler.observe_phase("session", SessionPhase::Idle);

        assert_eq!(actor.phase(), SessionPhase::Idle);
    }
}

use super::InboxCommandService;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, Weak},
};
use tokio::sync::Mutex as AsyncMutex;

#[derive(Clone, Default)]
pub(crate) struct InboxScheduler(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    sessions: HashMap<String, Arc<AsyncMutex<()>>>,
    running: HashMap<String, bool>,
    stopped: bool,
    initial_inputs: HashSet<String>,
    // AppState owns delivery; a weak link prevents the event/scheduler/delivery cycle from leaking.
    delivery: Weak<InboxCommandService>,
}

impl InboxScheduler {
    pub(crate) fn connect(&self, delivery: &Arc<InboxCommandService>) {
        self.0.lock().expect("inbox scheduler lock").delivery = Arc::downgrade(delivery);
    }

    pub fn begin_initial(&self, session: &str) {
        self.0
            .lock()
            .expect("inbox scheduler lock")
            .initial_inputs
            .insert(session.into());
    }
    pub fn finish_initial(&self, session: &str) {
        self.0
            .lock()
            .expect("inbox scheduler lock")
            .initial_inputs
            .remove(session);
    }
    pub fn awaiting_initial(&self, session: &str) -> bool {
        self.0
            .lock()
            .expect("inbox scheduler lock")
            .initial_inputs
            .contains(session)
    }

    pub fn session_lock(&self, session: &str) -> Arc<AsyncMutex<()>> {
        self.0
            .lock()
            .expect("inbox scheduler lock")
            .sessions
            .entry(session.into())
            .or_default()
            .clone()
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
        state.running.insert(session.clone(), false);
        let scheduler = self.clone();
        tokio::spawn(async move {
            loop {
                if let Err(error) = inbox.drain_inbox(&session).await {
                    tracing::warn!(%session, %error, "Inbox scheduling failed");
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
    pub(crate) async fn reserve_initial_input(
        &self,
        session: &str,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        self.scheduler.session_lock(session).lock_owned().await
    }

    pub fn notify_available(&self, session: &str) {
        self.scheduler.wake(session.into());
    }

    pub async fn recover_deliveries(&self) -> pontia_core::Result<()> {
        sqlx::query("UPDATE inbox_messages SET state='failed',failure_message='Delivery is uncertain after Pontia restarted; input was not retried',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE state='dispatching'")
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn resume_pending(&self) -> pontia_core::Result<()> {
        let sessions: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT session_id FROM inbox_messages WHERE state='pending'",
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

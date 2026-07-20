use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use pontia_core::Result;
use serde_json::Value;
use tokio::sync::Notify;

const DEFAULT_TTL: Duration = Duration::from_secs(10 * 60);
const DEFAULT_CAPACITY: usize = 4_096;

#[derive(Debug, Clone, PartialEq)]
pub struct IdempotencyOutcome {
    pub data: Value,
    pub duplicate: bool,
}

/// Provides process-local, short-lived idempotency for command responses.
///
/// Completed responses are forgotten after the configured TTL and all entries are lost on
/// process restart. When capacity is fully occupied by in-flight operations, new keys execute
/// without caching rather than growing memory without bound.
#[derive(Clone)]
pub struct IdempotencyCoordinator {
    inner: Arc<CoordinatorInner>,
}

struct CoordinatorInner {
    entries: Mutex<HashMap<CacheKey, Arc<Entry>>>,
    ttl: Duration,
    capacity: usize,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    operation: String,
    key: String,
}

struct Entry {
    created_at: Instant,
    state: Mutex<EntryState>,
    changed: Notify,
}

enum EntryState {
    Processing,
    Completed { data: Value, expires_at: Instant },
    Failed,
}

enum Reservation {
    Owner(Arc<Entry>),
    Existing(Arc<Entry>),
    Uncached,
}

struct OwnerGuard {
    entry: Arc<Entry>,
    armed: bool,
}

impl OwnerGuard {
    fn new(entry: Arc<Entry>) -> Self {
        Self { entry, armed: true }
    }

    fn complete(mut self, data: Value, ttl: Duration) {
        *self
            .entry
            .state
            .lock()
            .expect("idempotency entry lock poisoned") = EntryState::Completed {
            data,
            expires_at: Instant::now() + ttl,
        };
        self.armed = false;
        self.entry.changed.notify_waiters();
    }
}

impl Drop for OwnerGuard {
    fn drop(&mut self) {
        if self.armed {
            *self
                .entry
                .state
                .lock()
                .expect("idempotency entry lock poisoned") = EntryState::Failed;
            self.entry.changed.notify_waiters();
        }
    }
}

impl IdempotencyCoordinator {
    pub fn new(ttl: Duration, capacity: usize) -> Self {
        assert!(
            capacity > 0,
            "idempotency capacity must be greater than zero"
        );
        Self {
            inner: Arc::new(CoordinatorInner {
                entries: Mutex::new(HashMap::new()),
                ttl,
                capacity,
            }),
        }
    }

    /// Executes `action` once per `(operation, key)` while its result remains cached.
    /// Concurrent duplicates wait for the owner; failed or cancelled owners may be retried.
    pub async fn run<F, Fut>(
        &self,
        operation: impl Into<String>,
        key: Option<&str>,
        action: F,
    ) -> Result<IdempotencyOutcome>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value>>,
    {
        let Some(key) = key else {
            return action().await.map(|data| IdempotencyOutcome {
                data,
                duplicate: false,
            });
        };

        let cache_key = CacheKey {
            operation: operation.into(),
            key: key.to_owned(),
        };
        let mut action = Some(action);

        loop {
            match self.reserve(&cache_key) {
                Reservation::Owner(entry) => {
                    let guard = OwnerGuard::new(entry);
                    let data = action
                        .take()
                        .expect("idempotency action can only be owned once")(
                    )
                    .await?;
                    guard.complete(data.clone(), self.inner.ttl);
                    return Ok(IdempotencyOutcome {
                        data,
                        duplicate: false,
                    });
                }
                Reservation::Existing(entry) => {
                    let changed = entry.changed.notified();
                    let completed = {
                        let state = entry.state.lock().expect("idempotency entry lock poisoned");
                        match &*state {
                            EntryState::Completed { data, expires_at }
                                if *expires_at > Instant::now() =>
                            {
                                Some(data.clone())
                            }
                            EntryState::Processing => None,
                            EntryState::Completed { .. } | EntryState::Failed => continue,
                        }
                    };
                    if let Some(data) = completed {
                        return Ok(IdempotencyOutcome {
                            data,
                            duplicate: true,
                        });
                    }
                    changed.await;
                }
                Reservation::Uncached => {
                    return action
                        .take()
                        .expect("idempotency action can only be executed once")(
                    )
                    .await
                    .map(|data| IdempotencyOutcome {
                        data,
                        duplicate: false,
                    });
                }
            }
        }
    }

    fn reserve(&self, key: &CacheKey) -> Reservation {
        let now = Instant::now();
        let mut entries = self
            .inner
            .entries
            .lock()
            .expect("idempotency cache lock poisoned");

        if let Some(entry) = entries.get(key) {
            let reusable = match &*entry.state.lock().expect("idempotency entry lock poisoned") {
                EntryState::Processing => true,
                EntryState::Completed { expires_at, .. } => *expires_at > now,
                EntryState::Failed => false,
            };
            if reusable {
                return Reservation::Existing(entry.clone());
            }
            entries.remove(key);
        }

        entries.retain(|_, entry| {
            match &*entry.state.lock().expect("idempotency entry lock poisoned") {
                EntryState::Processing => true,
                EntryState::Completed { expires_at, .. } => *expires_at > now,
                EntryState::Failed => false,
            }
        });

        if entries.len() >= self.inner.capacity {
            let oldest_completed = entries
                .iter()
                .filter_map(|(key, entry)| {
                    matches!(
                        &*entry.state.lock().expect("idempotency entry lock poisoned"),
                        EntryState::Completed { .. }
                    )
                    .then_some((key.clone(), entry.created_at))
                })
                .min_by_key(|(_, created_at)| *created_at)
                .map(|(key, _)| key);
            if let Some(oldest_completed) = oldest_completed {
                entries.remove(&oldest_completed);
            } else {
                return Reservation::Uncached;
            }
        }

        let entry = Arc::new(Entry {
            created_at: now,
            state: Mutex::new(EntryState::Processing),
            changed: Notify::new(),
        });
        entries.insert(key.clone(), entry.clone());
        Reservation::Owner(entry)
    }
}

impl Default for IdempotencyCoordinator {
    fn default() -> Self {
        Self::new(DEFAULT_TTL, DEFAULT_CAPACITY)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use pontia_core::error::Error;
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn replays_a_completed_result() {
        let coordinator = IdempotencyCoordinator::default();
        let calls = AtomicUsize::new(0);

        let first = coordinator
            .run("create_session", Some("key"), || async {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(json!({ "attempt": 1 }))
            })
            .await
            .unwrap();
        let second = coordinator
            .run("create_session", Some("key"), || async {
                calls.fetch_add(1, Ordering::Relaxed);
                Ok(json!({ "attempt": 2 }))
            })
            .await
            .unwrap();

        assert!(!first.duplicate);
        assert!(second.duplicate);
        assert_eq!(second.data, first.data);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn concurrent_requests_share_one_execution() {
        let coordinator = IdempotencyCoordinator::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let release = Arc::new(Notify::new());

        let first = tokio::spawn({
            let coordinator = coordinator.clone();
            let calls = calls.clone();
            let release = release.clone();
            async move {
                coordinator
                    .run("operation", Some("key"), || async move {
                        calls.fetch_add(1, Ordering::Relaxed);
                        release.notified().await;
                        Ok(json!({ "ok": true }))
                    })
                    .await
                    .unwrap()
            }
        });

        while calls.load(Ordering::Relaxed) == 0 {
            tokio::task::yield_now().await;
        }

        let second = tokio::spawn({
            let coordinator = coordinator.clone();
            let calls = calls.clone();
            async move {
                coordinator
                    .run("operation", Some("key"), || async move {
                        calls.fetch_add(1, Ordering::Relaxed);
                        Ok(json!({ "unexpected": true }))
                    })
                    .await
                    .unwrap()
            }
        });
        tokio::task::yield_now().await;
        release.notify_one();

        let first = first.await.unwrap();
        let second = second.await.unwrap();
        assert!(!first.duplicate);
        assert!(second.duplicate);
        assert_eq!(first.data, second.data);
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn capacity_stays_bounded_when_all_entries_are_processing() {
        let coordinator = IdempotencyCoordinator::new(Duration::from_secs(60), 1);
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let first = tokio::spawn({
            let coordinator = coordinator.clone();
            let started = started.clone();
            let release = release.clone();
            async move {
                coordinator
                    .run("first", Some("key"), || async move {
                        started.notify_one();
                        release.notified().await;
                        Ok(json!(1))
                    })
                    .await
                    .unwrap()
            }
        });
        started.notified().await;

        let second = coordinator
            .run("second", Some("key"), || async { Ok(json!(2)) })
            .await
            .unwrap();
        assert!(!second.duplicate);
        assert_eq!(
            coordinator
                .inner
                .entries
                .lock()
                .expect("idempotency cache lock poisoned")
                .len(),
            1
        );

        release.notify_one();
        first.await.unwrap();
    }

    #[tokio::test]
    async fn failed_execution_can_be_retried() {
        let coordinator = IdempotencyCoordinator::default();
        let first = coordinator
            .run("operation", Some("key"), || async {
                Err(Error::Domain("failed".to_owned()))
            })
            .await;
        assert!(first.is_err());

        let second = coordinator
            .run("operation", Some("key"), || async {
                Ok(json!({ "ok": true }))
            })
            .await
            .unwrap();
        assert!(!second.duplicate);
    }

    #[tokio::test]
    async fn expired_results_execute_again() {
        let coordinator = IdempotencyCoordinator::new(Duration::from_millis(5), 10);
        coordinator
            .run("operation", Some("key"), || async { Ok(json!(1)) })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        let result = coordinator
            .run("operation", Some("key"), || async { Ok(json!(2)) })
            .await
            .unwrap();
        assert!(!result.duplicate);
        assert_eq!(result.data, json!(2));
    }
}

use std::time::Duration;

use pontia_storage_sqlite::repositories::events::SqliteEventRepository;
use serde_json::Value;
use sqlx::SqlitePool;
use tokio::time::{Instant, sleep};

use pontia_core::error::{Error, Result};

const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_READY_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub struct RuntimeReadinessService {
    pool: SqlitePool,
    timeout: Duration,
    poll_interval: Duration,
}

impl RuntimeReadinessService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            timeout: DEFAULT_READY_TIMEOUT,
            poll_interval: DEFAULT_READY_POLL_INTERVAL,
        }
    }

    pub fn with_options(pool: SqlitePool, timeout: Duration, poll_interval: Duration) -> Self {
        Self {
            pool,
            timeout,
            poll_interval,
        }
    }

    pub async fn is_ready(
        &self,
        session_id: &str,
        client_type: &str,
        runtime_instance_id: &str,
    ) -> Result<bool> {
        let payloads = SqliteEventRepository::new(self.pool.clone())
            .ready_payloads(session_id, client_type)
            .await?;

        for payload in payloads {
            let value: Value = serde_json::from_str(&payload)?;
            if value
                .get("runtime_instance_id")
                .and_then(Value::as_str)
                .is_some_and(|value| value == runtime_instance_id)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub async fn wait_until_ready(
        &self,
        session_id: &str,
        client_type: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        let deadline = Instant::now() + self.timeout;
        loop {
            if self
                .is_ready(session_id, client_type, runtime_instance_id)
                .await?
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::Domain(
                    "agent client did not report session.ready before timeout".to_string(),
                ));
            }
            sleep(self.poll_interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pontia_core::{
        domain::{EventSource, EventType, ReportedEvent},
        ids::new_event_id,
    };

    use crate::EventIngestService;
    use pontia_storage_sqlite::{connect_sqlite, run_migrations};
    use serde_json::json;

    async fn pool() -> SqlitePool {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("readiness.db");
        let _kept_dir = dir.keep();
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        db
    }

    #[tokio::test]
    async fn readiness_matches_current_runtime_instance_id() {
        let pool = pool().await;
        let service = EventIngestService::new(pool.clone());
        service
            .ingest_event(ReportedEvent::new(
                new_event_id().to_string(),
                "sess_ready".to_string(),
                None,
                EventSource::RuntimeManager,
                "pi".to_string(),
                EventType::SessionReady,
                json!({"runtime_instance_id":"rtinst_new"}),
            ))
            .await
            .unwrap();
        service
            .ingest_event(ReportedEvent::new(
                new_event_id().to_string(),
                "sess_ready".to_string(),
                None,
                EventSource::AgentClient,
                "pi".to_string(),
                EventType::SessionReady,
                json!({"runtime_instance_id":"rtinst_old"}),
            ))
            .await
            .unwrap();

        let readiness = RuntimeReadinessService::with_options(
            pool,
            Duration::from_millis(10),
            Duration::from_millis(1),
        );

        assert!(
            !readiness
                .is_ready("sess_ready", "pi", "rtinst_new")
                .await
                .unwrap()
        );
        assert!(
            readiness
                .is_ready("sess_ready", "pi", "rtinst_old")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn readiness_wait_times_out_clearly() {
        let pool = pool().await;
        let readiness = RuntimeReadinessService::with_options(
            pool,
            Duration::from_millis(5),
            Duration::from_millis(1),
        );

        let error = readiness
            .wait_until_ready("sess_missing", "pi", "rtinst_missing")
            .await
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("agent client did not report session.ready before timeout")
        );
    }
}

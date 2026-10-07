use std::time::Duration;

use pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository;
use sqlx::SqlitePool;
use tokio::time::{Instant, sleep};

use pontia_core::error::{Error, Result};

const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(30);
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
        runtime_id: &str,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM session_runtimes r JOIN sessions s USING(session_id) WHERE r.runtime_id = ? AND r.session_id = ? AND r.role = 'tui' AND r.state = 'running' AND s.client_type = ?)")
            .bind(runtime_id).bind(session_id).bind(client_type).fetch_one(&self.pool).await?)
    }

    pub async fn wait_until_bound_and_ready(
        &self,
        session_id: &str,
        client_type: &str,
    ) -> Result<String> {
        let deadline = Instant::now() + self.timeout;
        loop {
            if let Some(runtime_id) = SqliteSessionRuntimeRepository::new(self.pool.clone())
                .runtime_id(session_id)
                .await?
                && self.is_ready(session_id, client_type, &runtime_id).await?
            {
                return Ok(runtime_id);
            }
            if Instant::now() >= deadline {
                return Err(Error::Domain(
                    "agent client did not confirm its runtime binding and report session.ready before timeout"
                        .to_string(),
                ));
            }
            sleep(self.poll_interval).await;
        }
    }

    pub async fn wait_until_ready(
        &self,
        session_id: &str,
        client_type: &str,
        runtime_id: &str,
    ) -> Result<()> {
        let deadline = Instant::now() + self.timeout;
        loop {
            if self.is_ready(session_id, client_type, runtime_id).await? {
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
    use pontia_storage_sqlite::{connect_sqlite, run_migrations};

    async fn pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("readiness.db");
        let database_url = format!("sqlite://{}", db_path.display());
        let db = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&db).await.expect("migrate");
        (db, dir)
    }

    #[tokio::test]
    async fn readiness_uses_current_state_when_reusing_a_runtime() {
        let (pool, _root) = pool().await;
        sqlx::query(
            "INSERT INTO sessions(session_id,client_type,state) VALUES ('session','pi','starting')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO session_runtimes(runtime_id,session_id,role,state,created_at) VALUES ('runtime','session','tui','starting','2000-01-01T00:00:00Z')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO events(event_id,session_id,client_type,event_type,source,occurred_at,payload) VALUES ('historical-ready','session','pi','session.ready','agent_client','2000-01-01T00:00:00Z','{\"runtime_id\":\"runtime\"}')").execute(&pool).await.unwrap();
        let readiness = RuntimeReadinessService::new(pool.clone());
        assert!(
            !readiness
                .is_ready("session", "pi", "runtime")
                .await
                .unwrap()
        );
        sqlx::query("UPDATE session_runtimes SET state='running' WHERE runtime_id='runtime'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            readiness
                .is_ready("session", "pi", "runtime")
                .await
                .unwrap()
        );
        sqlx::query("UPDATE session_runtimes SET state='starting' WHERE runtime_id='runtime'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            !readiness
                .is_ready("session", "pi", "runtime")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn readiness_wait_times_out_clearly() {
        let (pool, _pontia_home) = pool().await;
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

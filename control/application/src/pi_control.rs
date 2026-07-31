use pontia_agent_clients::{TerminateBehavior, get_client_spec};
use pontia_core::{Error, Result};
use pontia_runtime::GenericRuntimeManager;
use pontia_storage_sqlite::repositories::{
    runtime_bindings::SqliteRuntimeBindingRepository, sessions::SqliteSessionRepository,
};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct PiGracefulExitService {
    pool: SqlitePool,
    runtime: Arc<dyn PiExitTransport>,
}

trait PiExitTransport: Send + Sync {
    fn send_keys(&self, socket_path: &str, pane_id: &str, keys: &[&str]) -> Result<()>;
}

impl PiExitTransport for GenericRuntimeManager {
    fn send_keys(&self, socket_path: &str, pane_id: &str, keys: &[&str]) -> Result<()> {
        self.send_tmux_keys(socket_path, pane_id, keys)
    }
}

impl PiGracefulExitService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: Arc::new(GenericRuntimeManager),
        }
    }

    #[cfg(test)]
    fn with_transport(pool: SqlitePool, runtime: Arc<dyn PiExitTransport>) -> Self {
        Self { pool, runtime }
    }

    pub async fn request_exit(&self, session_id: &str, runtime_instance_id: &str) -> Result<()> {
        let (socket_path, pane_id) = self
            .validated_control_target(session_id, runtime_instance_id)
            .await?;
        let keys = match get_client_spec("pi").map(|spec| spec.adapter.terminate) {
            Some(TerminateBehavior::TmuxSendKeys(keys)) => keys,
            _ => {
                return Err(Error::CapabilityUnavailable(
                    "pi graceful exit is unavailable".to_string(),
                ));
            }
        };
        self.runtime.send_keys(&socket_path, &pane_id, keys)
    }

    pub async fn ensure_current_runtime(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<()> {
        self.validated_control_target(session_id, runtime_instance_id)
            .await?;
        Ok(())
    }

    async fn validated_control_target(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
    ) -> Result<(String, String)> {
        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if session.client_type != "pi" {
            return Err(Error::CapabilityUnavailable(format!(
                "session {session_id} is not a pi session"
            )));
        }
        let binding = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .tmux_pane_binding(session_id)
            .await?
            .ok_or_else(|| {
                Error::CapabilityUnavailable(format!(
                    "session {session_id} has no current runtime binding"
                ))
            })?;
        if binding.runtime_instance_id.as_deref() != Some(runtime_instance_id) {
            return Err(Error::StateConflict(format!(
                "runtime {runtime_instance_id} is not the current runtime for session {session_id}"
            )));
        }
        binding
            .socket_path
            .zip(binding.pane_id)
            .filter(|(socket_path, pane_id)| {
                !socket_path.trim().is_empty() && !pane_id.trim().is_empty()
            })
            .ok_or_else(|| {
                Error::CapabilityUnavailable(format!(
                    "session {session_id} has no bound pi TUI pane"
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use pontia_storage_sqlite::{
        connect_sqlite,
        repositories::runtime_bindings::{
            RuntimeBindingUpsertRecord, SqliteRuntimeBindingRepository,
        },
        run_migrations,
    };

    use super::*;

    #[derive(Default)]
    struct RecordingTransport {
        sent: Mutex<Vec<(String, String, Vec<String>)>>,
    }

    impl PiExitTransport for RecordingTransport {
        fn send_keys(&self, socket_path: &str, pane_id: &str, keys: &[&str]) -> Result<()> {
            self.sent.lock().expect("sent keys lock").push((
                socket_path.to_string(),
                pane_id.to_string(),
                keys.iter().map(|key| (*key).to_string()).collect(),
            ));
            Ok(())
        }
    }

    #[tokio::test]
    async fn graceful_exit_sends_pi_sequence_without_persisting_session_exit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let database_url = format!("sqlite://{}", temp.path().join("pi-exit.db").display());
        let pool = connect_sqlite(&database_url).await.expect("connect");
        run_migrations(&pool).await.expect("migrate");
        sqlx::query(
            "INSERT INTO sessions (session_id, client_type, state) VALUES ('sess_pi', 'pi', 'working')",
        )
        .execute(&pool)
        .await
        .expect("create pi session");
        SqliteRuntimeBindingRepository::new(pool.clone())
            .upsert_binding(RuntimeBindingUpsertRecord {
                session_id: "sess_pi".to_string(),
                runtime_kind: "pi_tui".to_string(),
                runtime_instance_id: Some("rtinst_pi".to_string()),
                start_command: None,
                launch_cwd: None,
                last_seen_at: None,
                tmux_socket_path: Some("/tmp/pontia-test.sock".to_string()),
                tmux_pane_id: Some("%7".to_string()),
                metadata: "{}".to_string(),
            })
            .await
            .expect("bind runtime");
        let transport = Arc::new(RecordingTransport::default());
        let service = PiGracefulExitService::with_transport(pool.clone(), transport.clone());

        service
            .request_exit("sess_pi", "rtinst_pi")
            .await
            .expect("request graceful exit");
        let stale_error = service
            .request_exit("sess_pi", "rtinst_stale")
            .await
            .expect_err("stale runtime must not receive exit keys");
        assert!(stale_error.to_string().contains("current runtime"));

        assert_eq!(
            transport.sent.lock().expect("sent keys lock").as_slice(),
            &[(
                "/tmp/pontia-test.sock".to_string(),
                "%7".to_string(),
                vec!["C-c".to_string(), "C-c".to_string()]
            )]
        );
        let exit_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE session_id = 'sess_pi' AND event_type = 'session.exited'",
        )
        .fetch_one(&pool)
        .await
        .expect("count session exit events");
        assert_eq!(exit_events, 0);
        let state: String =
            sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = 'sess_pi'")
                .fetch_one(&pool)
                .await
                .expect("load session state");
        assert_eq!(state, "working");
    }
}
use std::sync::Arc;

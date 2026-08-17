use std::time::Duration;

use serde_json::json;
use sqlx::SqlitePool;
use tokio::sync::watch;

use pontia_agent_clients::{RuntimeBehavior, get_client_spec};
use pontia_core::error::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, TmuxProcessFingerprint};
use pontia_storage_sqlite::repositories::{
    runtime_bindings::{ActiveTmuxProcessBindingRow, SqliteRuntimeBindingRepository},
    sessions::SqliteSessionRepository,
    turns::SqliteTurnRepository,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AgentEventBroker, EventIngestService, ExternalQueryService, PontiaEvent, PontiaEventSource,
    PontiaEventType,
};

const PROCESS_OBSERVATION_INTERVAL: Duration = Duration::from_secs(10);
const PROCESS_OBSERVATION_RETRY_DELAY: Duration = Duration::from_secs(1);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const STARTUP_TIMEOUT_SWEEP_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone)]
pub struct RuntimeObservationService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
    agent_events: Option<AgentEventBroker>,
}

impl RuntimeObservationService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
            agent_events: None,
        }
    }

    pub fn with_agent_events(mut self, agent_events: AgentEventBroker) -> Self {
        self.agent_events = Some(agent_events);
        self
    }

    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        let mut process_interval = tokio::time::interval(PROCESS_OBSERVATION_INTERVAL);
        process_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Tokio intervals tick immediately. Fingerprints are captured by the
        // binding path, so the first validation is intentionally delayed 10s.
        process_interval.tick().await;
        let mut startup_interval = tokio::time::interval(STARTUP_TIMEOUT_SWEEP_INTERVAL);
        startup_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = process_interval.tick() => {
                    if let Err(error) = self.sweep_active_tmux_sessions().await {
                        tracing::warn!(%error, "runtime process fingerprint sweep failed");
                    }
                }
                _ = startup_interval.tick() => {
                    if let Err(error) = self.sweep_startup_timeouts().await {
                        tracing::warn!(%error, "runtime startup timeout sweep failed");
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    }

    pub async fn sweep_startup_timeouts(&self) -> Result<()> {
        let cutoff = (OffsetDateTime::now_utc() - STARTUP_TIMEOUT)
            .format(&Rfc3339)
            .map_err(|error| Error::Domain(format!("invalid startup timeout cutoff: {error}")))?;
        let sessions = SqliteSessionRepository::new(self.pool.clone())
            .starting_sessions_before(&cutoff)
            .await?;

        for session in sessions {
            let timeout_seconds = STARTUP_TIMEOUT.as_secs();
            let mut payload = json!({
                "reason": "startup_timeout",
                "timeout_seconds": timeout_seconds,
                "failure": {
                    "message": format!(
                        "agent client did not report session.ready within {timeout_seconds} seconds"
                    )
                }
            });
            if let Some(runtime_instance_id) = session.runtime_instance_id {
                payload["runtime_instance_id"] = json!(runtime_instance_id);
            }
            let transitioned = self
                .ingest_service()
                .ingest_startup_timeout_event(PontiaEvent::new(
                    session.session_id.clone(),
                    None,
                    PontiaEventSource::RuntimeManager,
                    session.client_type,
                    PontiaEventType::SessionError,
                    payload,
                ))
                .await?;
            if transitioned
                && let Some(runtime_handle) = session.runtime_handle
                && let Err(error) = self.runtime.terminate_session(&runtime_handle)
            {
                tracing::warn!(
                    session_id = %session.session_id,
                    %error,
                    "failed to terminate runtime after startup timeout"
                );
            }
        }
        Ok(())
    }

    pub async fn sweep_active_tmux_sessions(&self) -> Result<()> {
        let bindings = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .active_tmux_process_bindings()
            .await?;
        for binding in bindings {
            if let Err(error) = self.observe_tmux_process(binding).await {
                tracing::warn!(%error, "tmux agent process observation failed");
            }
        }
        Ok(())
    }

    pub async fn observe_session(&self, session_id: &str) -> Result<()> {
        let query = ExternalQueryService::new(self.pool.clone());
        let session = query
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if matches!(session.state.as_str(), "exited" | "error") {
            return Ok(());
        }

        let Some(client_spec) = get_client_spec(&session.client_type) else {
            return Ok(());
        };
        match client_spec.adapter.runtime {
            RuntimeBehavior::Tmux(_) => {
                let repository = SqliteRuntimeBindingRepository::new(self.pool.clone());
                let Some(row) = repository.tmux_pane_binding(session_id).await? else {
                    return Ok(());
                };
                let Some((socket_path, pane_id)) =
                    row.socket_path
                        .zip(row.pane_id)
                        .filter(|(socket_path, pane_id)| {
                            !socket_path.trim().is_empty() && !pane_id.trim().is_empty()
                        })
                else {
                    return Ok(());
                };
                let Some(runtime_instance_id) = row.runtime_instance_id else {
                    return Ok(());
                };
                return self
                    .observe_tmux_process(ActiveTmuxProcessBindingRow {
                        session_id: session_id.to_string(),
                        client_type: session.client_type,
                        runtime_instance_id,
                        socket_path,
                        pane_id,
                        process_fingerprint: row.process_fingerprint,
                    })
                    .await;
            }
            RuntimeBehavior::InProcess => {
                let Some(runtime_target) = SqliteRuntimeBindingRepository::new(self.pool.clone())
                    .runtime_handle(session_id)
                    .await?
                else {
                    return Ok(());
                };
                if self.runtime.is_alive(&runtime_target) {
                    return Ok(());
                }
            }
        }

        self.record_runtime_error(session_id, &session.client_type)
            .await
    }

    async fn observe_tmux_process(&self, binding: ActiveTmuxProcessBindingRow) -> Result<()> {
        let Some(fingerprint) = binding
            .process_fingerprint
            .as_deref()
            .and_then(|value| serde_json::from_str::<TmuxProcessFingerprint>(value).ok())
        else {
            return self
                .record_process_exit(binding, "agent_process_fingerprint_unavailable")
                .await;
        };
        if self.runtime.validate_tmux_process_fingerprint(
            &binding.socket_path,
            &binding.pane_id,
            &fingerprint,
        ) {
            return Ok(());
        }

        tokio::time::sleep(PROCESS_OBSERVATION_RETRY_DELAY).await;
        if self.runtime.validate_tmux_process_fingerprint(
            &binding.socket_path,
            &binding.pane_id,
            &fingerprint,
        ) {
            return Ok(());
        }

        self.record_process_exit(binding, "agent_process_fingerprint_missing")
            .await
    }

    async fn record_process_exit(
        &self,
        binding: ActiveTmuxProcessBindingRow,
        reason: &str,
    ) -> Result<()> {
        self.ingest_service()
            .ingest_runtime_observation_event(PontiaEvent::new(
                binding.session_id,
                None,
                PontiaEventSource::RuntimeManager,
                binding.client_type,
                PontiaEventType::SessionExited,
                json!({
                    "runtime_instance_id": binding.runtime_instance_id,
                    "reason": reason,
                }),
            ))
            .await?;
        Ok(())
    }

    async fn record_runtime_error(&self, session_id: &str, client_type: &str) -> Result<()> {
        let ingest = self.ingest_service();
        if let Some(active_turn) = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?
        {
            ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session_id.to_string(),
                    Some(active_turn.turn_id),
                    PontiaEventSource::RuntimeManager,
                    client_type.to_string(),
                    PontiaEventType::TurnAbandoned,
                    json!({ "failure": { "message": "runtime is not alive" } }),
                ))
                .await?;
        }
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::RuntimeManager,
                client_type.to_string(),
                PontiaEventType::SessionError,
                json!({ "failure": { "message": "runtime is not alive" } }),
            ))
            .await?;
        Ok(())
    }

    fn ingest_service(&self) -> EventIngestService {
        let ingest = EventIngestService::new(self.pool.clone());
        match &self.agent_events {
            Some(agent_events) => ingest.with_agent_events(agent_events.clone()),
            None => ingest,
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
    use pontia_storage_sqlite::{connect_sqlite, run_migrations};

    async fn pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("startup-timeout.db");
        let pool = connect_sqlite(&format!("sqlite://{}", db_path.display()))
            .await
            .expect("connect");
        run_migrations(&pool).await.expect("migrate");
        (pool, dir)
    }

    async fn create_starting_session(pool: &SqlitePool, session_id: &str) {
        let ingest = EventIngestService::new(pool.clone());
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::ExternalApi,
                "pi".to_string(),
                PontiaEventType::SessionCreated,
                json!({}),
            ))
            .await
            .expect("create session");
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::ExternalApi,
                "pi".to_string(),
                PontiaEventType::SessionStarting,
                json!({}),
            ))
            .await
            .expect("start session");
    }

    #[tokio::test]
    async fn startup_timeout_transitions_starting_session_to_error() {
        let (pool, _dir) = pool().await;
        create_starting_session(&pool, "sess_timeout").await;
        sqlx::query(
            "UPDATE events SET created_at = '2000-01-01T00:00:00.000Z' WHERE session_id = 'sess_timeout' AND event_type = 'session.starting'",
        )
        .execute(&pool)
        .await
        .expect("age startup event");

        RuntimeObservationService::new(pool.clone())
            .sweep_startup_timeouts()
            .await
            .expect("sweep startup timeouts");

        let state: String =
            sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = 'sess_timeout'")
                .fetch_one(&pool)
                .await
                .expect("session state");
        let (reason, timeout_seconds): (String, i64) = sqlx::query_as(
            "SELECT json_extract(payload, '$.reason'), json_extract(payload, '$.timeout_seconds') FROM events WHERE session_id = 'sess_timeout' AND event_type = 'session.error'",
        )
        .fetch_one(&pool)
        .await
        .expect("timeout event");

        assert_eq!(state, "error");
        assert_eq!(reason, "startup_timeout");
        assert_eq!(timeout_seconds, 30);
    }

    #[tokio::test]
    async fn startup_timeout_does_not_change_session_that_became_ready() {
        let (pool, _dir) = pool().await;
        create_starting_session(&pool, "sess_ready").await;
        EventIngestService::new(pool.clone())
            .ingest_reported_event(ReportedEvent::new(
                new_event_id().to_string(),
                "sess_ready".to_string(),
                None,
                EventSource::AgentClient,
                "pi".to_string(),
                EventType::SessionReady,
                json!({}),
            ))
            .await
            .expect("ready session");

        RuntimeObservationService::new(pool.clone())
            .sweep_startup_timeouts()
            .await
            .expect("sweep startup timeouts");

        let state: String =
            sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = 'sess_ready'")
                .fetch_one(&pool)
                .await
                .expect("session state");
        let timeout_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE session_id = 'sess_ready' AND event_type = 'session.error'",
        )
        .fetch_one(&pool)
        .await
        .expect("timeout event count");

        assert_eq!(state, "idle");
        assert_eq!(timeout_events, 0);
    }
}

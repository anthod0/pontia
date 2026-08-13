use std::time::Duration;

use serde_json::{Value, json};
use sqlx::SqlitePool;
use tokio::sync::watch;

use pontia_agent_clients::{RuntimeBehavior, get_client_spec};
use pontia_core::error::{Error, Result};
use pontia_runtime::{GenericRuntimeManager, TmuxProcessFingerprint};
use pontia_storage_sqlite::repositories::{
    runtime_bindings::{ActiveTmuxProcessBindingRow, SqliteRuntimeBindingRepository},
    turns::SqliteTurnRepository,
};

use crate::{
    AgentEventBroker, EventIngestService, ExternalQueryService, PontiaEvent, PontiaEventSource,
    PontiaEventType,
};

const PROCESS_OBSERVATION_INTERVAL: Duration = Duration::from_secs(10);
const PROCESS_OBSERVATION_RETRY_DELAY: Duration = Duration::from_secs(1);

fn runtime_target_from_metadata(metadata: Value) -> Option<String> {
    metadata["in_process"]["runtime_handle"]
        .as_str()
        .or_else(|| metadata["in_process"]["runtime_key"].as_str())
        .map(ToString::to_string)
}

fn process_fingerprint(metadata: &str) -> Option<TmuxProcessFingerprint> {
    serde_json::from_str::<Value>(metadata)
        .ok()?
        .get("tmux_process_fingerprint")
        .cloned()
        .and_then(|fingerprint| serde_json::from_value(fingerprint).ok())
}

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
        let mut interval = tokio::time::interval(PROCESS_OBSERVATION_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // Tokio intervals tick immediately. Fingerprints are captured by the
        // binding path, so the first validation is intentionally delayed 10s.
        interval.tick().await;

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(error) = self.sweep_active_tmux_sessions().await {
                        tracing::warn!(%error, "runtime process fingerprint sweep failed");
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
                let Some(metadata) = repository.metadata(session_id).await? else {
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
                        metadata,
                    })
                    .await;
            }
            RuntimeBehavior::InProcess => {
                let metadata = SqliteRuntimeBindingRepository::new(self.pool.clone())
                    .metadata(session_id)
                    .await?;
                let Some(runtime_target) = metadata
                    .map(|metadata| {
                        serde_json::from_str::<Value>(&metadata).map(runtime_target_from_metadata)
                    })
                    .transpose()?
                    .flatten()
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
        let Some(fingerprint) = process_fingerprint(&binding.metadata) else {
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

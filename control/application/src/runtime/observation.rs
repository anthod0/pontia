use serde_json::{Value, json};
use sqlx::SqlitePool;

use pontia_agent_clients::{RuntimeBehavior, get_client_spec};
use pontia_core::error::{Error, Result};
use pontia_runtime::GenericRuntimeManager;
use pontia_storage_sqlite::repositories::{
    runtime_bindings::SqliteRuntimeBindingRepository, turns::SqliteTurnRepository,
};

use crate::{
    EventIngestService, ExternalQueryService, PontiaEvent, PontiaEventSource, PontiaEventType,
};

fn runtime_target_from_metadata(metadata: Value) -> Option<String> {
    metadata["in_process"]["runtime_handle"]
        .as_str()
        .or_else(|| metadata["in_process"]["runtime_key"].as_str())
        .map(ToString::to_string)
}

#[derive(Clone)]
pub struct RuntimeObservationService {
    pool: SqlitePool,
    runtime: GenericRuntimeManager,
}

impl RuntimeObservationService {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            runtime: GenericRuntimeManager,
        }
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
                let Some(row) = SqliteRuntimeBindingRepository::new(self.pool.clone())
                    .tmux_pane_binding(session_id)
                    .await?
                else {
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
                if self.runtime.is_tmux_pane_alive(&socket_path, &pane_id) {
                    return Ok(());
                }
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

        let ingest = EventIngestService::new(self.pool.clone());
        if let Some(active_turn) = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await?
        {
            ingest
                .ingest_pontia_event(PontiaEvent::new(
                    session_id.to_string(),
                    Some(active_turn.turn_id),
                    PontiaEventSource::RuntimeManager,
                    session.client_type.clone(),
                    PontiaEventType::TurnAbandoned,
                    json!({ "failure": { "message": "runtime tmux session is not alive" } }),
                ))
                .await?;
        }
        ingest
            .ingest_pontia_event(PontiaEvent::new(
                session_id.to_string(),
                None,
                PontiaEventSource::RuntimeManager,
                session.client_type,
                PontiaEventType::SessionError,
                json!({ "failure": { "message": "runtime tmux session is not alive" } }),
            ))
            .await?;
        Ok(())
    }
}

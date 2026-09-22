use pontia_core::{
    Error, Result,
    domain::{DomainEvent, EventSource, EventType},
};
use pontia_storage_sqlite::repositories::{
    runtime_bindings::SqliteRuntimeBindingRepository, sessions::SqliteSessionRepository,
};
use serde_json::json;

use super::EventIngestService;
use crate::{PontiaEvent, PontiaEventSource, PontiaEventType};

pub(super) async fn existing_reporting_failure_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    event: &DomainEvent,
) -> Result<Option<String>> {
    if event.event_type != EventType::SessionError
        || event.source != EventSource::RuntimeManager
        || event.payload["reason"] != "turn_start_reporting_failed"
    {
        return Ok(None);
    }
    let Some(runtime_instance_id) = event.payload["runtime_instance_id"].as_str() else {
        return Ok(None);
    };
    pontia_storage_sqlite::repositories::events::SqliteEventRepository::turn_start_reporting_failure_in_tx(
        tx, &event.session_id, runtime_instance_id,
    ).await
}

impl EventIngestService {
    /// Records a failed integration operation, not an inferred agent execution
    /// failure. Persisting the Session fact also covers reports that arrive
    /// before a Workflow has finished binding its newly created Session.
    pub async fn report_turn_start_failure(
        &self,
        session_id: &str,
        runtime_instance_id: &str,
        reason: &str,
    ) -> Result<()> {
        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        let runtime = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .runtime_instance_id(session_id)
            .await?;
        if runtime.as_deref() != Some(runtime_instance_id) {
            return Err(Error::StateConflict(
                "reporting failure does not match the current confirmed runtime".into(),
            ));
        }
        let reason: String = reason.chars().take(200).collect();
        self.ingest_runtime_observation_event(PontiaEvent::new(
            session_id,
            None,
            PontiaEventSource::RuntimeManager,
            session.client_type,
            PontiaEventType::SessionError,
            json!({
                "runtime_instance_id": runtime_instance_id,
                "reason": "turn_start_reporting_failed",
                "failure": { "message": format!("turn.started reporting failed: {reason}") },
            }),
        ))
        .await?;
        Ok(())
    }
}

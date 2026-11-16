use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::{
    session_runtimes::SqliteSessionRuntimeRepository, turns::SqliteTurnRepository,
};
use sqlx::{Sqlite, Transaction};

use super::{
    RuntimeBindingUpsertRequest, request::non_empty, service::RuntimeBindingUpsertService,
};
use crate::AgentBindingService;

impl RuntimeBindingUpsertService {
    pub(super) async fn ensure_existing_binding_agrees(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        if let Some(binding) = AgentBindingService::new(self.pool.clone())
            .binding_for_client_session(&request.client_type, &request.client_session_key)
            .await?
            && binding.session_id != session_id
        {
            return Err(Error::StateConflict(format!(
                "runtime binding update does not match session {session_id} Agent binding"
            )));
        }
        Ok(())
    }

    pub(super) async fn ensure_active_runtime_is_not_replaced(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
        process: &pontia_runtime::ProcessFingerprint,
    ) -> Result<()> {
        let state: String = sqlx::query_scalar("SELECT state FROM sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&self.pool)
            .await?;
        if state == "exited" {
            return Ok(());
        }

        let Some(binding) = AgentBindingService::new(self.pool.clone())
            .binding_for_session(session_id)
            .await?
        else {
            // The Control Plane may have created the runtime before the TUI has
            // confirmed its native client identity for the first time.
            return Ok(());
        };
        let existing_runtime_id = SqliteSessionRuntimeRepository::new(self.pool.clone())
            .runtime_id(
                session_id,
                self.clients.control_runtime_role(&request.client_type),
            )
            .await?;
        let same_runtime = non_empty(request.runtime_id.as_deref())
            .zip(existing_runtime_id.as_deref())
            .is_some_and(|(incoming, existing)| incoming == existing);
        let same_client = binding.client_type == request.client_type
            && binding.client_session_key == request.client_session_key;

        let existing_process = match existing_runtime_id {
            Some(runtime_id) => SqliteSessionRuntimeRepository::new(self.pool.clone())
                .get(&runtime_id)
                .await?
                .and_then(|record| record.process_fingerprint),
            None => None,
        };
        let same_process = existing_process
            .as_deref()
            .map(serde_json::from_str::<pontia_runtime::ProcessFingerprint>)
            .transpose()?
            .is_some_and(|existing| existing == *process);
        if same_client
            && same_runtime
            && (same_process || (state == "starting" && existing_process.is_none()))
        {
            return Ok(());
        }
        Err(Error::StateConflict(format!(
            "session {session_id} already has an active Pontia-managed agent TUI"
        )))
    }
}

pub(super) async fn fence_runtime_binding_write(
    tx: &mut Transaction<'_, Sqlite>,
    session_id: &str,
    runtime_id: Option<&str>,
    role: Option<&str>,
) -> Result<()> {
    SqliteTurnRepository::serialize_session_turn_writes_in_tx(tx, session_id).await?;
    SqliteSessionRuntimeRepository::ensure_runtime_owner_may_write_in_tx(
        tx, session_id, runtime_id, role,
    )
    .await
}

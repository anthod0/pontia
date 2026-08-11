use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::{
    runtime_bindings::SqliteRuntimeBindingRepository, turns::SqliteTurnRepository,
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
        let existing_runtime_instance_id = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .runtime_instance_id(session_id)
            .await?;
        let existing_tmux = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .tmux_pane_binding(session_id)
            .await?;
        let incoming_tmux = request.tmux.as_ref().and_then(|tmux| {
            Some((
                non_empty(tmux.socket_path.as_deref())?,
                non_empty(tmux.pane_id.as_deref())?,
            ))
        });
        let same_runtime = non_empty(request.runtime_instance_id.as_deref())
            .zip(existing_runtime_instance_id.as_deref())
            .is_some_and(|(incoming, existing)| incoming == existing);
        let same_pane = match (existing_tmux, incoming_tmux) {
            (Some(existing), Some((incoming_socket, incoming_pane))) => {
                existing.socket_path.as_deref() == Some(incoming_socket.as_str())
                    && existing.pane_id.as_deref() == Some(incoming_pane.as_str())
            }
            (None, None) => true,
            _ => false,
        };
        let same_client = binding.client_type == request.client_type
            && binding.client_session_key == request.client_session_key;

        if same_client && same_runtime && same_pane {
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
    runtime_instance_id: Option<&str>,
) -> Result<()> {
    SqliteTurnRepository::serialize_session_turn_writes_in_tx(tx, session_id).await?;
    SqliteRuntimeBindingRepository::ensure_runtime_owner_may_write_in_tx(
        tx,
        session_id,
        runtime_instance_id,
    )
    .await
}

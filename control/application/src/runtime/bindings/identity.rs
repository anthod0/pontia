use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::{
    agent_bindings::SqliteAgentBindingRepository, sessions::SqliteSessionRepository,
};

use super::{
    RuntimeBindingUpsertRequest, request::non_empty, service::RuntimeBindingUpsertService,
};
use crate::AgentBindingService;

impl RuntimeBindingUpsertService {
    pub(super) async fn session_id_for_client_session(
        &self,
        client_type: &str,
        client_session_key: &str,
    ) -> Result<Option<String>> {
        SqliteAgentBindingRepository::new(self.pool.clone())
            .session_id_for_client_session(client_type, client_session_key)
            .await
    }

    pub(super) async fn unbound_session_id_for_client_session(
        &self,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<Option<String>> {
        if request.client_type == "pi"
            && let Some(session_id) = sqlx::query_scalar(
                r#"SELECT s.session_id
                   FROM sessions s
                   LEFT JOIN agent_bindings a ON a.session_id = s.session_id
                   WHERE s.session_id = ? AND s.client_type = ? AND a.id IS NULL"#,
            )
            .bind(&request.client_session_key)
            .bind(&request.client_type)
            .fetch_optional(&self.pool)
            .await?
        {
            return Ok(Some(session_id));
        }

        let Some(tmux) = request.tmux.as_ref() else {
            return Ok(None);
        };
        let Some(socket_path) = non_empty(tmux.socket_path.as_deref()) else {
            return Ok(None);
        };
        let Some(pane_id) = non_empty(tmux.pane_id.as_deref()) else {
            return Ok(None);
        };
        Ok(sqlx::query_scalar(
            r#"SELECT s.session_id
               FROM sessions s
               JOIN runtime_bindings r ON r.session_id = s.session_id
               LEFT JOIN agent_bindings a ON a.session_id = s.session_id
               WHERE s.client_type = ?
                 AND s.state != 'exited'
                 AND a.id IS NULL
                 AND r.tmux_socket_path = ?
                 AND r.tmux_pane_id = ?
                 AND COALESCE(json_extract(r.metadata, '$.binding_confirmed'), 0) = 0"#,
        )
        .bind(&request.client_type)
        .bind(socket_path)
        .bind(pane_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub(super) async fn unconfirmed_runtime_instance_id_for_pane(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<Option<String>> {
        let Some(tmux) = request.tmux.as_ref() else {
            return Ok(None);
        };
        let Some(socket_path) = non_empty(tmux.socket_path.as_deref()) else {
            return Ok(None);
        };
        let Some(pane_id) = non_empty(tmux.pane_id.as_deref()) else {
            return Ok(None);
        };
        Ok(sqlx::query_scalar(
            r#"SELECT runtime_instance_id
               FROM runtime_bindings
               WHERE session_id = ?
                 AND tmux_socket_path = ?
                 AND tmux_pane_id = ?
                 AND COALESCE(json_extract(metadata, '$.binding_confirmed'), 0) = 0"#,
        )
        .bind(session_id)
        .bind(socket_path)
        .bind(pane_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten())
    }

    pub(super) async fn ensure_requested_session(
        &self,
        session_id: &str,
        request: &RuntimeBindingUpsertRequest,
    ) -> Result<()> {
        let session = SqliteSessionRepository::new(self.pool.clone())
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if session.client_type != request.client_type {
            return Err(Error::StateConflict(format!(
                "session {session_id} uses client_type {}, not {}",
                session.client_type, request.client_type
            )));
        }
        if let Some(owner) = AgentBindingService::new(self.pool.clone())
            .binding_for_client_session(&request.client_type, &request.client_session_key)
            .await?
            && owner.session_id != session_id
        {
            return Err(Error::StateConflict(format!(
                "client session identity is already bound to session {}",
                owner.session_id
            )));
        }
        Ok(())
    }
}

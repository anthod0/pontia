use super::ControlQueryService;
use crate::runtime::shutdown::ShutdownSession;
use pontia_core::{Result, domain::SessionState};
use pontia_storage_sqlite::repositories::agent_clients::SqliteAgentClientRepository;
use pontia_storage_sqlite::repositories::native_turn_bindings::SqliteNativeTurnBindingRepository;
pub use pontia_storage_sqlite::repositories::session_runtimes::SessionRuntimeRecord;
use pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository;
pub use pontia_storage_sqlite::repositories::sessions::SessionLaunchFields;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;
use serde_json::Value;

#[derive(sqlx::FromRow)]
pub struct ClientSessionFields {
    pub client_type: String,
    pub state: SessionState,
    pub workspace_ref: Option<String>,
    pub metadata: Value,
    pub client_session_key: Option<String>,
}

impl ControlQueryService {
    pub async fn client_session_fields(
        &self,
        session: &str,
    ) -> Result<Option<ClientSessionFields>> {
        Ok(sqlx::query_as(
            "SELECT s.client_type, s.state, s.workspace_ref, s.metadata, b.client_session_key
             FROM sessions s LEFT JOIN agent_bindings b USING(session_id)
             WHERE s.session_id=?",
        )
        .bind(session)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn non_exited_bound_session(
        &self,
        client: &str,
        native_session: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT b.session_id FROM agent_bindings b JOIN sessions s USING(session_id)
             WHERE b.client_type=? AND b.client_session_key=? AND s.state<>'exited'",
        )
        .bind(client)
        .bind(native_session)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn client_turn_id(&self, session: &str, turn: &str) -> Result<Option<String>> {
        SqliteNativeTurnBindingRepository::new(self.pool.clone())
            .client_turn_id(session, turn)
            .await
    }

    pub async fn enabled_client_versions(
        &self,
        client_type: &str,
    ) -> Result<Vec<(String, Option<String>)>> {
        let records = SqliteAgentClientRepository::new(self.pool.clone())
            .list()
            .await?;
        Ok(records
            .into_iter()
            .filter(|row| row.enabled && row.client_type == client_type)
            .map(|row| (row.client_key, row.version))
            .collect())
    }

    pub async fn session_state(&self, session: &str) -> Result<Option<SessionState>> {
        SqliteSessionRepository::new(self.pool.clone())
            .get_state(session)
            .await
    }

    pub async fn session_metadata(&self, session: &str) -> Result<Option<Value>> {
        SqliteSessionRepository::new(self.pool.clone())
            .get_metadata(session)
            .await
    }

    pub async fn session_launch_settings(
        &self,
        session: &str,
    ) -> Result<Option<SessionLaunchFields>> {
        SqliteSessionRepository::new(self.pool.clone())
            .get_launch_fields(session)
            .await
    }

    pub async fn session_runtimes(&self, session: &str) -> Result<Vec<SessionRuntimeRecord>> {
        SqliteSessionRuntimeRepository::new(self.pool.clone())
            .list(session)
            .await
    }

    pub async fn session_runtime(&self, runtime: &str) -> Result<Option<SessionRuntimeRecord>> {
        SqliteSessionRuntimeRepository::new(self.pool.clone())
            .get(runtime)
            .await
    }

    /// Empty states include all states. The optional filter matches a top-level string metadata field.
    pub async fn list_sessions_by_client(
        &self,
        client_type: &str,
        states: &[SessionState],
        metadata: Option<(&str, &str)>,
    ) -> Result<Vec<String>> {
        SqliteSessionRepository::new(self.pool.clone())
            .list_by_client_type(client_type, states, metadata)
            .await
    }

    pub async fn list_active_shutdown_sessions(
        &self,
        client_type: &str,
    ) -> Result<Vec<ShutdownSession>> {
        let rows = SqliteSessionRepository::new(self.pool.clone())
            .list_active_shutdown_sessions(client_type)
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| ShutdownSession {
                session_id: row.session_id,
                title: row.title,
                client_type: row.client_type,
                workspace: row.workspace,
            })
            .collect())
    }
}

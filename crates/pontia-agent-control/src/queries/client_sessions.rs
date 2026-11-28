use super::ControlQueryService;
use crate::runtime::shutdown::ShutdownSession;
use pontia_core::{Result, domain::SessionState};
pub use pontia_storage_sqlite::repositories::session_runtimes::SessionRuntimeRecord;
use pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository;
pub use pontia_storage_sqlite::repositories::sessions::SessionLaunchFields;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;
use serde_json::Value;

impl ControlQueryService {
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

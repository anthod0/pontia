use sqlx::SqlitePool;

use crate::models::sessions::{RuntimeBindingMetadataRow, SessionRow};

use pontia_core::Result;

#[derive(Debug, Clone)]
pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_sessions(&self) -> Result<Vec<SessionRow>> {
        Ok(sqlx::query_as::<_, SessionRow>(
            r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                      s.execution_profile_id, s.execution_profile_version,
                      s.state, s.current_turn_id, s.workspace_id,
                      COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                      s.metadata, s.created_at, s.updated_at
               FROM sessions s
               LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
               ORDER BY s.created_at, s.session_id"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SessionRow>> {
        Ok(sqlx::query_as::<_, SessionRow>(
            r#"SELECT s.session_id, s.client_type, s.title, s.handle, s.role, s.description,
                      s.execution_profile_id, s.execution_profile_version,
                      s.state, s.current_turn_id, s.workspace_id,
                      COALESCE(w.canonical_path, s.workspace_ref) AS workspace_ref,
                      s.metadata, s.created_at, s.updated_at
               FROM sessions s
               LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
               WHERE s.session_id = ?"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_runtime_binding_metadata(
        &self,
        session_id: &str,
    ) -> Result<Option<RuntimeBindingMetadataRow>> {
        Ok(sqlx::query_as::<_, RuntimeBindingMetadataRow>(
            "SELECT metadata FROM runtime_bindings WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn active_session_id_for_handle(
        &self,
        workspace_id: &str,
        handle: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            "SELECT session_id FROM sessions WHERE workspace_id = ? AND handle = ? AND state NOT IN ('exited', 'error') LIMIT 1",
        )
        .bind(workspace_id)
        .bind(handle)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn update_session_workspace(
        &self,
        session_id: &str,
        workspace_ref: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET workspace_ref = ?, workspace_id = ? WHERE session_id = ?")
            .bind(workspace_ref)
            .bind(workspace_id)
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

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
}

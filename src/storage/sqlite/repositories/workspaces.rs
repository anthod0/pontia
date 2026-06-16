use sqlx::SqlitePool;

use crate::{error::Result, storage::sqlite::models::workspaces::WorkspaceRow};

#[derive(Debug, Clone)]
pub struct SqliteWorkspaceRepository {
    pool: SqlitePool,
}

impl SqliteWorkspaceRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn list_workspaces(&self) -> Result<Vec<WorkspaceRow>> {
        Ok(sqlx::query_as::<_, WorkspaceRow>(
            r#"SELECT workspace_id, canonical_path, display_path, name, state, metadata,
                      created_at, updated_at, last_used_at
               FROM workspaces
               WHERE state != 'deleted'
               ORDER BY last_used_at DESC, created_at DESC, workspace_id"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRow>> {
        Ok(sqlx::query_as::<_, WorkspaceRow>(
            r#"SELECT workspace_id, canonical_path, display_path, name, state, metadata,
                      created_at, updated_at, last_used_at
               FROM workspaces WHERE workspace_id = ?"#,
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?)
    }
}

use sqlx::SqlitePool;

use crate::models::workspaces::WorkspaceRow;

use pontia_core::Result;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceRecordRow {
    pub workspace_id: String,
    pub canonical_path: String,
}

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

    pub async fn get_workspace_record(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceRecordRow>> {
        Ok(sqlx::query_as::<_, WorkspaceRecordRow>(
            "SELECT workspace_id, canonical_path FROM workspaces WHERE workspace_id = ?",
        )
        .bind(workspace_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn get_workspace_record_by_canonical_path(
        &self,
        canonical_path: &str,
    ) -> Result<Option<WorkspaceRecordRow>> {
        Ok(sqlx::query_as::<_, WorkspaceRecordRow>(
            "SELECT workspace_id, canonical_path FROM workspaces WHERE canonical_path = ?",
        )
        .bind(canonical_path)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn insert_workspace(
        &self,
        workspace_id: &str,
        canonical_path: &str,
        display_path: &str,
        name: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO workspaces
               (workspace_id, canonical_path, display_path, name, last_used_at)
               VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))"#,
        )
        .bind(workspace_id)
        .bind(canonical_path)
        .bind(display_path)
        .bind(name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn reactivate_workspace(
        &self,
        workspace_id: &str,
        display_path: &str,
        name: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            r#"UPDATE workspaces
               SET display_path = ?, name = COALESCE(?, name), state = 'active',
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                   last_used_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workspace_id = ?"#,
        )
        .bind(display_path)
        .bind(name)
        .bind(workspace_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn rename_workspace(&self, workspace_id: &str, name: Option<&str>) -> Result<u64> {
        Ok(sqlx::query(
            r#"UPDATE workspaces
               SET name = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workspace_id = ?"#,
        )
        .bind(name)
        .bind(workspace_id)
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn mark_deleted(&self, workspace_id: &str) -> Result<u64> {
        Ok(sqlx::query(
            r#"UPDATE workspaces
               SET state = 'deleted', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE workspace_id = ?"#,
        )
        .bind(workspace_id)
        .execute(&self.pool)
        .await?
        .rows_affected())
    }

    pub async fn active_workspace_exists_at_path(&self, canonical_path: &str) -> Result<bool> {
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workspaces WHERE canonical_path = ? AND state != 'deleted'",
        )
        .bind(canonical_path)
        .fetch_one(&self.pool)
        .await?
            > 0)
    }
}

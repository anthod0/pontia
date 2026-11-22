use sqlx::SqlitePool;

use crate::models::workspaces::WorkspaceRow;

use pontia_core::Result;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceRecordRow {
    pub workspace_id: String,
    pub canonical_path: String,
    pub name: Option<String>,
    pub description: Option<String>,
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
            r#"SELECT workspace_id, canonical_path, display_path, name, description, state, metadata,
                      created_at, updated_at, last_used_at
               FROM workspaces
               WHERE state = 'active'
               ORDER BY last_used_at DESC, created_at DESC, workspace_id"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn get_workspace(&self, workspace_id: &str) -> Result<Option<WorkspaceRow>> {
        Ok(sqlx::query_as::<_, WorkspaceRow>(
            r#"SELECT workspace_id, canonical_path, display_path, name, description, state, metadata,
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
            "SELECT workspace_id, canonical_path, name, description FROM workspaces WHERE workspace_id = ?",
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
            "SELECT workspace_id, canonical_path, name, description FROM workspaces WHERE canonical_path = ?",
        )
        .bind(canonical_path)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn register_workspace(
        &self,
        workspace_id: &str,
        canonical_path: &str,
        name: Option<&str>,
    ) -> Result<(WorkspaceRecordRow, Option<String>)> {
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await?;
        let previous_state =
            sqlx::query_scalar("SELECT state FROM workspaces WHERE canonical_path = ?")
                .bind(canonical_path)
                .fetch_optional(&mut *tx)
                .await?;
        let row = sqlx::query_as(
            r#"INSERT INTO workspaces (workspace_id, canonical_path, display_path, name, last_used_at)
               VALUES (?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
               ON CONFLICT(canonical_path) DO UPDATE SET display_path = excluded.display_path,
                 name = COALESCE(excluded.name, workspaces.name), state = 'active',
                 last_used_at = excluded.last_used_at, updated_at = excluded.last_used_at
               RETURNING workspace_id, canonical_path, name, description"#,
        )
        .bind(workspace_id).bind(canonical_path).bind(canonical_path).bind(name)
        .fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok((row, previous_state))
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

    pub async fn update_description(
        &self,
        workspace_id: &str,
        description: Option<&str>,
    ) -> Result<u64> {
        Ok(sqlx::query(
            "UPDATE workspaces SET description = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE workspace_id = ?",
        )
        .bind(description)
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
}

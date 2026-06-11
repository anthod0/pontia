use super::*;

impl SessionCommandService {
    pub(super) async fn idempotency_response(
        &self,
        operation: &str,
        key: &str,
    ) -> Result<Option<Value>> {
        let response: Option<String> = sqlx::query_scalar(
            "SELECT response FROM idempotency_keys WHERE operation = ? AND key = ?",
        )
        .bind(operation)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        response
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(Into::into)
    }

    pub(super) async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO idempotency_keys (operation, key, response)
               VALUES (?, ?, ?)
               ON CONFLICT(operation, key) DO NOTHING"#,
        )
        .bind(operation)
        .bind(key)
        .bind(serde_json::to_string(response)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn ensure_handle_available(
        &self,
        workspace_id: &str,
        handle: &str,
    ) -> Result<()> {
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT session_id FROM sessions WHERE workspace_id = ? AND handle = ? AND state NOT IN ('exited', 'error') LIMIT 1",
        )
        .bind(workspace_id)
        .bind(handle)
        .fetch_optional(&self.pool)
        .await?;

        if existing.is_some() {
            return Err(Error::Conflict {
                code: "session_handle_conflict",
                message: format!(
                    "Cannot create session because {handle} is already used, please try a different handle."
                ),
            });
        }

        Ok(())
    }

    pub(super) async fn upsert_runtime_binding(
        &self,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO runtime_bindings (session_id, runtime_kind, runtime_ref, metadata)
               VALUES (?, ?, ?, ?)
               ON CONFLICT(session_id) DO UPDATE SET
                   runtime_kind = excluded.runtime_kind,
                   runtime_ref = excluded.runtime_ref,
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')"#,
        )
        .bind(session_id)
        .bind(&runtime.runtime_kind)
        .bind(&runtime.runtime_ref)
        .bind(serde_json::to_string(&runtime.binding_metadata())?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub(super) async fn update_session_workspace(
        &self,
        session_id: &str,
        workspace: Option<&WorkspaceRecord>,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET workspace_ref = ?, workspace_id = ? WHERE session_id = ?")
            .bind(workspace.map(|workspace| workspace.canonical_path.as_str()))
            .bind(workspace.map(|workspace| workspace.workspace_id.as_str()))
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

use super::SqliteSessionRepository;
use pontia_core::{Result, domain::SessionState};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, sqlx::FromRow)]
pub struct SessionLaunchFields {
    pub workspace_ref: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    #[sqlx(json)]
    pub runtime_environment: BTreeMap<String, String>,
    pub credential_env: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
pub struct ShutdownSessionRow {
    pub session_id: String,
    pub title: Option<String>,
    pub client_type: String,
    pub workspace: Option<String>,
}

impl SqliteSessionRepository {
    pub async fn get_state(&self, session: &str) -> Result<Option<SessionState>> {
        Ok(
            sqlx::query_scalar("SELECT state FROM sessions WHERE session_id=?")
                .bind(session)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn get_metadata(&self, session: &str) -> Result<Option<Value>> {
        Ok(
            sqlx::query_scalar("SELECT metadata FROM sessions WHERE session_id=?")
                .bind(session)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn get_launch_fields(&self, session: &str) -> Result<Option<SessionLaunchFields>> {
        Ok(sqlx::query_as(
            "SELECT workspace_ref,metadata ->> '$.model' AS model,
                metadata ->> '$.reasoning_effort' AS reasoning_effort,runtime_environment,credential_env
             FROM sessions WHERE session_id=?",
        )
            .bind(session)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn list_by_client_type(
        &self,
        client_type: &str,
        states: &[SessionState],
        metadata: Option<(&str, &str)>,
    ) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT session_id FROM sessions s WHERE client_type=?
             AND (json_array_length(?2)=0 OR state IN (SELECT value FROM json_each(?2)))
             AND (?3 IS NULL OR EXISTS (SELECT 1 FROM json_each(s.metadata) WHERE key=?3 AND value=?4))
             ORDER BY session_id",
        )
            .bind(client_type)
            .bind(serde_json::to_string(states)?)
            .bind(metadata.map(|(key, _)| key))
            .bind(metadata.map(|(_, value)| value))
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn list_active_shutdown_sessions(
        &self,
        client_type: &str,
    ) -> Result<Vec<ShutdownSessionRow>> {
        Ok(sqlx::query_as(
            "SELECT s.session_id,s.title,s.client_type,COALESCE(w.display_path,s.workspace_ref) AS workspace
             FROM sessions s LEFT JOIN workspaces w USING(workspace_id)
             WHERE s.client_type=? AND s.state NOT IN ('exited','error') ORDER BY s.session_id",
        )
            .bind(client_type)
            .fetch_all(&self.pool)
            .await?)
    }
}

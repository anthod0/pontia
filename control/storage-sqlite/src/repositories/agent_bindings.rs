use pontia_core::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::models::agent_bindings::AgentBindingRow;

#[derive(Debug, Clone)]
pub struct AgentBindingUpsertRecord {
    pub id: String,
    pub session_id: String,
    pub client_type: String,
    pub launch_cwd: String,
    pub client_session_key: String,
    pub metadata: String,
}

#[derive(Debug, Clone)]
pub struct SqliteAgentBindingRepository {
    pool: SqlitePool,
}

impl SqliteAgentBindingRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn upsert_binding(
        &self,
        binding: AgentBindingUpsertRecord,
    ) -> Result<AgentBindingRow> {
        let mut tx = self.pool.begin().await?;
        let row = Self::upsert_binding_in_tx(&mut tx, binding).await?;
        tx.commit().await?;
        Ok(row)
    }

    pub async fn upsert_binding_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        binding: AgentBindingUpsertRecord,
    ) -> Result<AgentBindingRow> {
        Ok(sqlx::query_as::<_, AgentBindingRow>(
            r#"INSERT INTO agent_bindings
               (id, session_id, client_type, launch_cwd, client_session_key, metadata)
               VALUES (?, ?, ?, ?, ?, ?)
               ON CONFLICT(session_id, client_type, client_session_key) DO UPDATE SET
                   metadata = excluded.metadata,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               RETURNING id, session_id, client_type, launch_cwd, client_session_key, metadata, discovered"#,
        )
        .bind(binding.id)
        .bind(binding.session_id)
        .bind(binding.client_type)
        .bind(binding.launch_cwd)
        .bind(binding.client_session_key)
        .bind(binding.metadata)
        .fetch_one(&mut **tx)
        .await?)
    }

    pub async fn primary_binding_for_session(
        &self,
        session_id: &str,
    ) -> Result<Option<AgentBindingRow>> {
        Ok(sqlx::query_as::<_, AgentBindingRow>(
            r#"SELECT id, session_id, client_type, launch_cwd, client_session_key, metadata, discovered
               FROM agent_bindings
               WHERE session_id = ?
               ORDER BY created_at, id
               LIMIT 1"#,
        )
        .bind(session_id)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn mark_discovered(&self, binding_id: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE agent_bindings
               SET discovered = TRUE,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
               WHERE id = ? AND discovered = FALSE"#,
        )
        .bind(binding_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn session_id_for_client_session(
        &self,
        client_type: &str,
        client_session_key: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            r#"SELECT session_id
               FROM agent_bindings
               WHERE client_type = ? AND client_session_key = ?
               ORDER BY updated_at DESC, id DESC
               LIMIT 1"#,
        )
        .bind(client_type)
        .bind(client_session_key)
        .fetch_optional(&self.pool)
        .await?)
    }

    pub async fn latest_client_session_key(
        &self,
        session_id: &str,
        client_type: &str,
    ) -> Result<Option<String>> {
        Ok(sqlx::query_scalar(
            r#"SELECT client_session_key
               FROM agent_bindings
               WHERE session_id = ? AND client_type = ?
               ORDER BY updated_at DESC, id DESC
               LIMIT 1"#,
        )
        .bind(session_id)
        .bind(client_type)
        .fetch_optional(&self.pool)
        .await?)
    }
}

use pontia_core::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentClientRow {
    pub client_type: String,
    pub client_key: String,
    pub enabled: bool,
    pub version: Option<String>,
    pub version_checked_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct SqliteAgentClientRepository {
    pool: SqlitePool,
}

impl SqliteAgentClientRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get(&self, client_type: &str, client_key: &str) -> Result<Option<AgentClientRow>> {
        Ok(
            sqlx::query_as("SELECT client_type, client_key, enabled, version, version_checked_at, created_at, updated_at FROM agent_clients WHERE client_type = ? AND client_key = ?")
                .bind(client_type)
                .bind(client_key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn list(&self) -> Result<Vec<AgentClientRow>> {
        Ok(
            sqlx::query_as("SELECT client_type, client_key, enabled, version, version_checked_at, created_at, updated_at FROM agent_clients ORDER BY client_type, client_key")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn save(&self, client_type: &str, client_key: &str, enabled: bool) -> Result<()> {
        sqlx::query(
            "INSERT INTO agent_clients (client_type, client_key, enabled)
            VALUES (?, ?, ?)
            ON CONFLICT (client_type, client_key) DO UPDATE SET
                enabled = excluded.enabled,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        )
        .bind(client_type)
        .bind(client_key)
        .bind(enabled)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn begin_version_check(
        &self,
        client: &str,
        key: &str,
        started: &str,
        force: bool,
    ) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE agent_clients SET version_checked_at = ?
             WHERE client_type = ? AND client_key = ? AND enabled = 1
             AND (? OR version_checked_at IS NULL OR julianday(version_checked_at) <= julianday('now', '-15 minutes'))",
        )
        .bind(started)
        .bind(client)
        .bind(key)
        .bind(force)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn record_version(
        &self,
        client: &str,
        key: &str,
        started: &str,
        version: &str,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE agent_clients SET version = ? WHERE client_type = ? AND client_key = ?
             AND version_checked_at = ? AND enabled = 1",
        )
        .bind(version)
        .bind(client)
        .bind(key)
        .bind(started)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete(&self, client_type: &str, client_key: &str) -> Result<()> {
        sqlx::query("DELETE FROM agent_clients WHERE client_type = ? AND client_key = ?")
            .bind(client_type)
            .bind(client_key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

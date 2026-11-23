use pontia_core::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentClientRow {
    pub client_type: String,
    pub client_key: String,
    pub enabled: bool,
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
            sqlx::query_as("SELECT client_type, client_key, enabled, created_at, updated_at FROM agent_clients WHERE client_type = ? AND client_key = ?")
                .bind(client_type)
                .bind(client_key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn list(&self) -> Result<Vec<AgentClientRow>> {
        Ok(
            sqlx::query_as("SELECT client_type, client_key, enabled, created_at, updated_at FROM agent_clients ORDER BY client_type, client_key")
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

    pub async fn delete(&self, client_type: &str, client_key: &str) -> Result<()> {
        sqlx::query("DELETE FROM agent_clients WHERE client_type = ? AND client_key = ?")
            .bind(client_type)
            .bind(client_key)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

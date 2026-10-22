use pontia_core::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AgentClientRow {
    pub client_type: String,
    pub client_key: String,
    pub enabled: bool,
    pub config_overrides: String,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentClientRow {
    pub fn overrides(&self) -> Result<serde_json::Value> {
        Ok(serde_json::from_str(&self.config_overrides)?)
    }
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
            sqlx::query_as("SELECT * FROM agent_clients WHERE client_type = ? AND client_key = ?")
                .bind(client_type)
                .bind(client_key)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    pub async fn list(&self) -> Result<Vec<AgentClientRow>> {
        Ok(
            sqlx::query_as("SELECT * FROM agent_clients ORDER BY client_type, client_key")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    /// Replaces only supplied settings; new rows use the schema defaults.
    pub async fn save(
        &self,
        client_type: &str,
        client_key: &str,
        enabled: Option<bool>,
        config_overrides: Option<&str>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO agent_clients (client_type, client_key, enabled, config_overrides)
            VALUES (?, ?, COALESCE(?, 1), COALESCE(?, '{}'))
            ON CONFLICT (client_type, client_key) DO UPDATE SET
                enabled = COALESCE(?, agent_clients.enabled),
                config_overrides = COALESCE(?, agent_clients.config_overrides),
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')",
        )
        .bind(client_type)
        .bind(client_key)
        .bind(enabled)
        .bind(config_overrides)
        .bind(enabled)
        .bind(config_overrides)
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

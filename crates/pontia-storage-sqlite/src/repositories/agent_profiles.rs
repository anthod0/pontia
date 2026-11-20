use crate::models::agent_profiles::AgentProfileRow;
use pontia_core::Result;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct AgentProfileWriteRecord {
    pub profile_id: String,
    pub name: String,
    pub description: Option<String>,
    pub client_type: Option<String>,
    pub client_key: Option<String>,
    pub model: Option<String>,
    pub system_prompt: Option<String>,
}

#[derive(Clone)]
pub struct SqliteAgentProfileRepository {
    pool: SqlitePool,
}

impl SqliteAgentProfileRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn create(&self, record: AgentProfileWriteRecord) -> Result<()> {
        sqlx::query("INSERT INTO agent_profiles (profile_id, name, description, client_type, client_key, model, system_prompt) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(record.profile_id).bind(record.name).bind(record.description)
            .bind(record.client_type).bind(record.client_key).bind(record.model).bind(record.system_prompt)
            .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn update(&self, record: AgentProfileWriteRecord) -> Result<bool> {
        Ok(sqlx::query("UPDATE agent_profiles SET name=?, description=?, client_type=?, client_key=?, model=?, system_prompt=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE profile_id=?")
            .bind(record.name).bind(record.description).bind(record.client_type).bind(record.client_key)
            .bind(record.model).bind(record.system_prompt).bind(record.profile_id)
            .execute(&self.pool).await?.rows_affected() != 0)
    }

    pub async fn delete(&self, profile_id: &str) -> Result<bool> {
        Ok(sqlx::query("DELETE FROM agent_profiles WHERE profile_id=?")
            .bind(profile_id)
            .execute(&self.pool)
            .await?
            .rows_affected()
            != 0)
    }

    pub async fn list(&self) -> Result<Vec<AgentProfileRow>> {
        Ok(
            sqlx::query_as("SELECT * FROM agent_profiles ORDER BY profile_id")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    pub async fn get(&self, profile_id: &str) -> Result<Option<AgentProfileRow>> {
        Ok(
            sqlx::query_as("SELECT * FROM agent_profiles WHERE profile_id=?")
                .bind(profile_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }
}

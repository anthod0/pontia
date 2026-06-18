use pontia_core::Result;
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct SqliteIdempotencyRepository {
    pool: SqlitePool,
}

impl SqliteIdempotencyRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn get_response(&self, operation: &str, key: &str) -> Result<Option<Value>> {
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

    pub async fn store_response(&self, operation: &str, key: &str, response: &Value) -> Result<()> {
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
}

use super::*;
use crate::storage::sqlite::repositories::tasks::SqliteTaskRepository;

impl TaskCommandService {
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

    pub(super) async fn record_task_event(
        &self,
        task_id: &str,
        event_type: &str,
        payload: Value,
    ) -> Result<()> {
        let event_id = new_event_id().to_string();
        let payload = serde_json::to_string(&payload)?;
        SqliteTaskRepository::new(self.pool.clone())
            .record_task_event(&event_id, task_id, event_type, &payload)
            .await?;
        if self.graph.enabled
            && let Err(error) = GraphProjectionService::new(self.pool.clone(), self.graph.clone())
                .project_task(task_id)
                .await
        {
            tracing::warn!(task_id, event_type, error = %error, "graph projection failed");
        }
        Ok(())
    }
}

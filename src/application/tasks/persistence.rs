use super::*;
use pontia_storage_sqlite::repositories::{
    idempotency::SqliteIdempotencyRepository, tasks::SqliteTaskRepository,
};

impl TaskCommandService {
    pub(super) async fn idempotency_response(
        &self,
        operation: &str,
        key: &str,
    ) -> Result<Option<Value>> {
        SqliteIdempotencyRepository::new(self.pool.clone())
            .get_response(operation, key)
            .await
    }

    pub(super) async fn store_idempotency_response(
        &self,
        operation: &str,
        key: &str,
        response: &Value,
    ) -> Result<()> {
        SqliteIdempotencyRepository::new(self.pool.clone())
            .store_response(operation, key, response)
            .await
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

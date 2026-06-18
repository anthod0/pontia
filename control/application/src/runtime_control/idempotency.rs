use super::*;
use pontia_storage_sqlite::repositories::idempotency::SqliteIdempotencyRepository;

impl RuntimeControlService {
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
}

use serde::Deserialize;
use serde_json::Value;
use sqlx::SqlitePool;

use pontia_core::error::{Error, Result};
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;

#[derive(Debug, Clone, Deserialize)]
pub struct CurrentTurnClaimRequest {
    pub runtime_instance_id: String,
    pub client_type: String,
}

#[derive(Clone)]
pub struct CurrentTurnClaimService {
    pool: SqlitePool,
}

impl CurrentTurnClaimService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn claim(
        &self,
        session_id: &str,
        request: CurrentTurnClaimRequest,
    ) -> Result<Option<Value>> {
        let repository = SqliteRuntimeBindingRepository::new(self.pool.clone());
        let runtime_instance_id = repository
            .runtime_instance_id(session_id)
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "runtime binding for session {session_id} not found"
                ))
            })?;
        if runtime_instance_id != request.runtime_instance_id {
            return Err(Error::StateConflict(
                "runtime_instance_id does not match active runtime binding".to_string(),
            ));
        }
        repository
            .claim_pending_turn_context(
                session_id,
                &request.runtime_instance_id,
                &request.client_type,
            )
            .await?
            .map(|payload| serde_json::from_str(&payload).map_err(Into::into))
            .transpose()
    }
}

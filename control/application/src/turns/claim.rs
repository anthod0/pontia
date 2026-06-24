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
        let repo = SqliteRuntimeBindingRepository::new(self.pool.clone());
        let Some(metadata_json) = repo.metadata(session_id).await? else {
            return Err(Error::NotFound(format!(
                "runtime binding for session {session_id} not found"
            )));
        };
        let mut metadata: Value = serde_json::from_str(&metadata_json)?;
        if metadata["runtime_instance_id"].as_str() != Some(request.runtime_instance_id.as_str()) {
            return Err(Error::StateConflict(
                "runtime_instance_id does not match active runtime binding".to_string(),
            ));
        }
        let pending = metadata
            .get("pending_current_turn")
            .cloned()
            .filter(|value| {
                value.is_object()
                    && value["client_type"].as_str() == Some(request.client_type.as_str())
            });
        if pending.is_none() {
            return Ok(None);
        }
        if let Some(object) = metadata.as_object_mut() {
            object.remove("pending_current_turn");
        }
        repo.update_metadata(session_id, &serde_json::to_string(&metadata)?)
            .await?;
        Ok(pending)
    }
}

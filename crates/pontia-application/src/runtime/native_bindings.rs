use crate::UpsertAgentBindingRequest;
use pontia_core::{Error, Result};
use pontia_runtime::RuntimeStartResult;
use serde_json::Value;
use sqlx::SqlitePool;

pub(crate) struct NativeRuntimeBindings {
    pool: SqlitePool,
}
impl NativeRuntimeBindings {
    pub(crate) fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
    pub(crate) async fn provision(
        &self,
        session: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository::new(
            self.pool.clone(),
        )
        .upsert_binding(crate::runtime::runtime_binding_record(session, runtime)?)
        .await
    }

    pub(crate) async fn confirm(
        &self,
        binding: UpsertAgentBindingRequest,
        instance: &str,
        expected_instance: Option<&str>,
        capabilities: &crate::views::SessionCapabilities,
        details: Value,
    ) -> Result<()> {
        let session = binding.session_id.clone();
        let client = binding.client_type.clone();
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE runtime_bindings SET runtime_instance_id=?, binding_state='confirmed', capabilities=?, adapter_details=json_set(adapter_details,?,json(?)) WHERE session_id=? AND runtime_instance_id IS ?")
            .bind(instance).bind(serde_json::to_string(capabilities)?).bind(format!("$.{client}")).bind(details.to_string()).bind(&session).bind(expected_instance).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(Error::StateConflict(
                "Runtime binding changed before confirmation".into(),
            ));
        }
        crate::sessions::upsert_agent_binding_in_tx(&mut tx, binding).await?;
        tx.commit().await?;
        Ok(())
    }
}

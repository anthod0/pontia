use crate::UpsertAgentBindingRequest;
use pontia_core::{Error, Result};
use pontia_runtime::RuntimeStartResult;
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
        pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::new(
            self.pool.clone(),
        )
        .upsert_binding(crate::runtime::runtime_binding_record(session, runtime)?)
        .await
    }

    pub(crate) async fn confirm(
        &self,
        binding: UpsertAgentBindingRequest,
        runtime_id: &str,
    ) -> Result<()> {
        let session = binding.session_id.clone();
        let mut tx = self.pool.begin().await?;
        let current = pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository::runtime_id_in_tx(&mut tx, &session).await?;
        if current.as_deref() != Some(runtime_id) {
            return Err(Error::StateConflict(
                "Runtime identity cannot change during confirmation".into(),
            ));
        }
        crate::sessions::upsert_agent_binding_in_tx(&mut tx, binding).await?;
        tx.commit().await?;
        Ok(())
    }
}

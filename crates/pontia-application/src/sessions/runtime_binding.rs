use crate::runtime::runtime_binding_record;
use pontia_core::error::Result;
use pontia_runtime::RuntimeStartResult;
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;

use super::SessionCommandService;

impl SessionCommandService {
    pub(super) async fn start_command(&self, session_id: &str) -> Result<Option<String>> {
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .start_command(session_id)
            .await
    }

    pub(super) async fn restart_count(&self, session_id: &str) -> Result<Option<i64>> {
        SqliteRuntimeBindingRepository::new(self.pool.clone())
            .restart_count(session_id)
            .await
    }

    pub(super) async fn upsert_resumed_runtime_binding(
        &self,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        let record = runtime_binding_record(session_id, runtime)?;
        let result = SqliteRuntimeBindingRepository::new(self.pool.clone())
            .upsert_binding_guarded(record)
            .await;
        if result.is_err() {
            crate::clients::discard_unbound_runtime(runtime);
        }
        result
    }

    pub(super) async fn upsert_runtime_binding_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        let result = SqliteRuntimeBindingRepository::upsert_binding_in_tx(
            tx,
            runtime_binding_record(session_id, runtime)?,
        )
        .await;
        if result.is_err() {
            crate::clients::discard_unbound_runtime(runtime);
        }
        result
    }
}

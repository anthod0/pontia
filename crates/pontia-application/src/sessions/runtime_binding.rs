use crate::runtime::runtime_binding_record;
use pontia_core::error::Result;
use pontia_runtime::RuntimeStartResult;
use pontia_storage_sqlite::repositories::session_runtimes::SqliteSessionRuntimeRepository;

use super::SessionCommandService;

impl SessionCommandService {
    pub(super) async fn start_command(&self, session_id: &str) -> Result<Option<String>> {
        SqliteSessionRuntimeRepository::new(self.pool.clone())
            .start_command(session_id)
            .await
    }

    pub(super) async fn upsert_resumed_runtime_binding(
        &self,
        session_id: &str,
        runtime: &RuntimeStartResult,
    ) -> Result<()> {
        let record = runtime_binding_record(session_id, runtime)?;
        let result = SqliteSessionRuntimeRepository::new(self.pool.clone())
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
        let result = SqliteSessionRuntimeRepository::upsert_binding_in_tx(
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

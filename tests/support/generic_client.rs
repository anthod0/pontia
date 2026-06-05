use std::sync::{Arc, OnceLock};

use pilotfy::{
    adapters::{AdapterCapabilities, GenericTestAdapter},
    application::AppState,
    runtime::{AgentInput, GenericRuntimeManager},
};
use serde_json::Value;
use sqlx::Row;
use tokio::sync::{Mutex, OwnedMutexGuard};

pub struct GenericClientTestScope {
    _guard: OwnedMutexGuard<()>,
}

#[allow(dead_code)]
impl GenericClientTestScope {
    pub async fn new() -> Self {
        let guard = generic_test_lock().clone().lock_owned().await;
        GenericTestAdapter::clear_recorded_inputs();
        GenericRuntimeManager::reset_in_process_test_registry();
        Self { _guard: guard }
    }

    pub fn with_capabilities(self, capabilities: AdapterCapabilities) -> Self {
        GenericTestAdapter::set_capabilities(capabilities);
        self
    }

    pub fn auto_start_turn(self) -> Self {
        let mut behavior = GenericTestAdapter::behavior();
        behavior.auto_start_turn = true;
        GenericTestAdapter::set_behavior(behavior);
        self
    }

    pub fn write_current_turn_context(self) -> Self {
        let mut behavior = GenericTestAdapter::behavior();
        behavior.write_current_turn_context = true;
        GenericTestAdapter::set_behavior(behavior);
        self
    }

    pub fn recorded_inputs(&self) -> Vec<AgentInput> {
        GenericTestAdapter::recorded_inputs()
    }

    pub fn is_runtime_alive(&self, runtime_ref: &str) -> bool {
        GenericRuntimeManager.is_alive(runtime_ref)
    }

    pub fn reset_runtime_registry(&self) {
        GenericRuntimeManager::reset_in_process_test_registry();
    }

    pub async fn runtime_ref(&self, state: &AppState, session_id: &str) -> String {
        sqlx::query_scalar("SELECT runtime_ref FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db)
            .await
            .expect("runtime ref")
    }

    #[allow(dead_code)]
    pub async fn enable_builtin_profiles(&self, state: &AppState) {
        sqlx::query(
            r#"UPDATE execution_profiles
               SET supported_client_types = '["generic"]'
               WHERE profile_id IN ('default', 'planner', 'replanner', 'implementer', 'reviewer', 'tester', 'debugger')"#,
        )
        .execute(&state.db)
        .await
        .expect("enable generic builtin profiles");
    }

    pub async fn runtime_metadata(&self, state: &AppState, session_id: &str) -> Value {
        let row = sqlx::query("SELECT metadata FROM runtime_bindings WHERE session_id = ?")
            .bind(session_id)
            .fetch_one(&state.db)
            .await
            .expect("runtime binding");
        let metadata: String = row.try_get("metadata").expect("metadata");
        serde_json::from_str(&metadata).expect("metadata json")
    }
}

impl Drop for GenericClientTestScope {
    fn drop(&mut self) {
        GenericTestAdapter::clear_recorded_inputs();
        GenericRuntimeManager::reset_in_process_test_registry();
    }
}

fn generic_test_lock() -> &'static Arc<Mutex<()>> {
    static LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();
    LOCK.get_or_init(|| Arc::new(Mutex::new(())))
}

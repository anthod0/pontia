use std::sync::{Arc, OnceLock};

use pontia_application::AppState;
use pontia_application::client_contract::{AgentClientCapabilities, GenericTestClient};
use pontia_runtime::{AgentInput, GenericRuntimeManager};
use tokio::sync::{Mutex, OwnedMutexGuard};

pub(crate) struct GenericClientTestScope {
    _guard: OwnedMutexGuard<()>,
}

#[allow(dead_code)]
impl GenericClientTestScope {
    pub(crate) async fn new() -> Self {
        let guard = generic_test_lock().clone().lock_owned().await;
        GenericTestClient::clear_recorded_inputs();
        GenericRuntimeManager::reset_in_process_registry();
        Self { _guard: guard }
    }

    pub(crate) fn with_capabilities(self, capabilities: AgentClientCapabilities) -> Self {
        GenericTestClient::set_capabilities(capabilities);
        self
    }

    pub(crate) fn recorded_inputs(&self) -> Vec<AgentInput> {
        GenericTestClient::recorded_inputs()
    }

    pub(crate) fn is_runtime_alive(&self, runtime_handle: &str) -> bool {
        GenericRuntimeManager.is_alive(runtime_handle)
    }

    pub(crate) fn reset_runtime_registry(&self) {
        GenericRuntimeManager::reset_in_process_registry();
    }

    pub(crate) async fn runtime_handle(&self, state: &AppState, session_id: &str) -> String {
        sqlx::query_scalar("SELECT runtime_id FROM session_runtimes WHERE session_id=?")
            .bind(session_id)
            .fetch_one(&state.db())
            .await
            .unwrap()
    }

    #[allow(dead_code)]
    pub(crate) async fn enable_builtin_profiles(&self, state: &AppState) {
        sqlx::query(
            r#"UPDATE execution_profiles
               SET supported_client_types = '["generic"]'
               WHERE profile_id = 'default'"#,
        )
        .execute(&state.db())
        .await
        .expect("enable generic builtin profiles");
    }
}

impl Drop for GenericClientTestScope {
    fn drop(&mut self) {
        GenericTestClient::clear_recorded_inputs();
        GenericRuntimeManager::reset_in_process_registry();
    }
}

fn generic_test_lock() -> &'static Arc<Mutex<()>> {
    static LOCK: OnceLock<Arc<Mutex<()>>> = OnceLock::new();
    LOCK.get_or_init(|| Arc::new(Mutex::new(())))
}

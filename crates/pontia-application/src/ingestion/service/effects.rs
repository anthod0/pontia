use serde_json::Value;

use pontia_core::domain::{DomainEvent, EventType};
use pontia_runtime::GenericRuntimeManager;
use pontia_storage_sqlite::repositories::runtime_bindings::SqliteRuntimeBindingRepository;

pub(super) async fn clear_exited_session_tmux_markers(
    pool: &sqlx::SqlitePool,
    event: &DomainEvent,
    allow_bound_runtime_fallback: bool,
) {
    if event.event_type != EventType::SessionExited {
        return;
    }
    let repository = SqliteRuntimeBindingRepository::new(pool.clone());
    let runtime_instance_id = match event
        .payload
        .get("runtime_instance_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(runtime_instance_id) => runtime_instance_id.to_string(),
        None if allow_bound_runtime_fallback => {
            let Ok(Some(runtime_instance_id)) =
                repository.runtime_instance_id(&event.session_id).await
            else {
                return;
            };
            runtime_instance_id
        }
        None => return,
    };
    let Ok(Some(binding)) = repository.tmux_pane_binding(&event.session_id).await else {
        return;
    };
    let (Some(socket_path), Some(pane_id)) =
        (binding.socket_path.as_deref(), binding.pane_id.as_deref())
    else {
        return;
    };
    let _ = GenericRuntimeManager.clear_tmux_pane_markers(
        socket_path,
        pane_id,
        &event.session_id,
        &runtime_instance_id,
    );
}

use serde_json::Value;

use pontia_core::{
    domain::{DomainEvent, EventType},
    error::Result,
};
use pontia_runtime::GenericRuntimeManager;
use pontia_storage_sqlite::repositories::{
    inbox::SqliteInboxRepository, runtime_bindings::SqliteRuntimeBindingRepository,
};

use super::EventIngestService;

impl EventIngestService {
    pub(super) async fn clear_exited_session_tmux_markers(
        &self,
        event: &DomainEvent,
        allow_bound_runtime_fallback: bool,
    ) {
        if event.event_type != EventType::SessionExited {
            return;
        }
        let repository = SqliteRuntimeBindingRepository::new(self.pool.clone());
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

    pub(super) async fn link_started_turn_to_inbox_message(
        &self,
        event: &DomainEvent,
    ) -> Result<()> {
        if event.event_type != EventType::TurnStarted {
            return Ok(());
        }
        let Some(turn_id) = event.turn_id.as_deref() else {
            return Ok(());
        };
        let inbox_message_id = event
            .payload
            .pointer("/metadata/inbox_message_id")
            .or_else(|| event.payload.pointer("/input/inbox_message_id"))
            .and_then(Value::as_str);
        let Some(inbox_message_id) = inbox_message_id else {
            return Ok(());
        };

        SqliteInboxRepository::new(self.pool.clone())
            .link_started_turn(&event.session_id, inbox_message_id, turn_id)
            .await
    }
}

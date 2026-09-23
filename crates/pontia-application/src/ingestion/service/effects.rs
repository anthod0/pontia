use serde_json::Value;

use crate::{
    AgentEventBroker, ClientControlService, LiveOutputService,
    app::VolatileEventBroker,
    inbox::{InboxAssociations, InboxScheduler},
};
use pontia_core::{
    Result,
    domain::{DomainEvent, EventType},
};
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

#[derive(Clone, Default)]
pub(crate) struct PostCommitEffects {
    pub(super) client_control: Option<ClientControlService>,
    pub(super) agent_events: Option<AgentEventBroker>,
    pub(super) live_output: Option<LiveOutputService>,
    pub(super) volatile_events: Option<VolatileEventBroker>,
    pub(super) scheduler: InboxScheduler,
}

impl PostCommitEffects {
    pub(crate) fn new(
        client_control: ClientControlService,
        agent_events: AgentEventBroker,
        live_output: LiveOutputService,
        volatile_events: VolatileEventBroker,
        scheduler: InboxScheduler,
    ) -> Self {
        Self {
            client_control: Some(client_control),
            agent_events: Some(agent_events),
            live_output: Some(live_output),
            volatile_events: Some(volatile_events),
            scheduler,
        }
    }

    pub(super) async fn apply(&self, pool: &sqlx::SqlitePool, event: &DomainEvent) -> Result<()> {
        if matches!(
            event.event_type,
            EventType::SessionExited | EventType::SessionError
        ) && let Some(control) = &self.client_control
        {
            control.refresh_session(&event.session_id).await;
        }

        if let Some(agent_events) = &self.agent_events {
            agent_events.publish(event.clone());
        }
        if let Some(live_output) = &self.live_output {
            match event.event_type {
                EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnAbandoned
                | EventType::TurnInterrupted => {
                    if let Some(turn_id) = event.turn_id.as_deref() {
                        live_output.discard_turn(&event.session_id, turn_id);
                    }
                }
                EventType::SessionExited | EventType::SessionError => {
                    live_output.discard_session(&event.session_id);
                }
                _ => {}
            }
        }

        clear_exited_session_tmux_markers(pool, event, true).await;
        InboxAssociations::new(pool.clone())
            .observe_committed(&self.scheduler, event)
            .await?;
        Ok(())
    }
}

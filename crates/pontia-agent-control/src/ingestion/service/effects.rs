use crate::{
    AgentEventBroker, LiveOutputService,
    inbox::{InboxScheduler, SessionPhase},
};
use pontia_core::{
    Result,
    domain::{DomainEvent, EventType},
};

#[derive(Clone, Default)]
pub(crate) struct PostCommitEffects {
    pub(super) agent_events: Option<AgentEventBroker>,
    pub(super) live_output: Option<LiveOutputService>,
    pub(super) scheduler: InboxScheduler,
}

impl PostCommitEffects {
    pub(crate) fn new(
        agent_events: AgentEventBroker,
        live_output: LiveOutputService,
        scheduler: InboxScheduler,
    ) -> Self {
        Self {
            agent_events: Some(agent_events),
            live_output: Some(live_output),
            scheduler,
        }
    }

    pub(super) async fn apply(&self, event: &DomainEvent) -> Result<()> {
        if let Some(agent_events) = &self.agent_events {
            agent_events.publish(event.clone());
        }
        if let Some(live_output) = &self.live_output {
            match event.event_type {
                EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
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

        match event.event_type {
            EventType::TurnStarted => self
                .scheduler
                .observe_phase(&event.session_id, SessionPhase::Busy),
            EventType::TurnInterruptRequested => self
                .scheduler
                .observe_phase(&event.session_id, SessionPhase::Interrupting),
            EventType::SessionReady
            | EventType::InteractionUnblocked
            | EventType::SessionExited
            | EventType::SessionError
            | EventType::TurnCompleted
            | EventType::TurnFailed
            | EventType::TurnDispatchFailed
            | EventType::TurnInterrupted => self
                .scheduler
                .observe_phase(&event.session_id, SessionPhase::Idle),
            _ => {}
        }
        let manual_compaction_finished = event.payload["trigger"] == "manual"
            && matches!(
                event.event_type,
                EventType::SessionCompactionCompleted | EventType::SessionCompactionFailed
            );
        if manual_compaction_finished
            || matches!(
                event.event_type,
                EventType::SessionReady
                    | EventType::InteractionUnblocked
                    | EventType::TurnStarted
                    | EventType::TurnCompleted
                    | EventType::TurnFailed
                    | EventType::TurnDispatchFailed
                    | EventType::TurnInterrupted
            )
        {
            self.scheduler.wake(event.session_id.clone());
        }
        Ok(())
    }
}

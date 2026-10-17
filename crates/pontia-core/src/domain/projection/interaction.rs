use super::{ProjectionState, SessionState};
use crate::domain::{DomainEvent, EventType};

#[derive(Debug, Clone)]
pub(super) struct InteractionBlock {
    turn_id: Option<String>,
    blocked: bool,
    occurred_at: time::OffsetDateTime,
}

impl ProjectionState {
    /// Restores blocking facts from the durable Session event stream, in commit order.
    /// This does not replay native operations or modify the supplied projections.
    pub fn restore_interaction_history<'a>(
        &mut self,
        events: impl IntoIterator<Item = &'a DomainEvent>,
    ) {
        for event in events {
            self.observe_interaction_fact(event);
        }
    }

    pub(super) fn observe_interaction_fact(&mut self, event: &DomainEvent) {
        let blocks = self
            .interaction_blocks
            .entry(event.session_id.clone())
            .or_default();
        match event.event_type {
            EventType::InteractionBlocked | EventType::InteractionUnblocked => {
                let Some(id) = event
                    .payload
                    .get("interaction_id")
                    .and_then(|id| id.as_str())
                else {
                    return;
                };
                let terminal_turn = event
                    .turn_id
                    .as_ref()
                    .is_some_and(|id| self.interaction_terminal_turns.contains(id));
                let superseded = self
                    .interaction_terminal_facts
                    .get(&event.session_id)
                    .is_some_and(|terminal| *terminal >= event.occurred_at);
                if blocks
                    .get(id)
                    .is_some_and(|block| block.occurred_at > event.occurred_at)
                {
                    return;
                }
                blocks.insert(
                    id.to_string(),
                    InteractionBlock {
                        turn_id: event.turn_id.clone(),
                        blocked: event.event_type == EventType::InteractionBlocked
                            && !terminal_turn
                            && !superseded,
                        occurred_at: event.occurred_at,
                    },
                );
            }
            EventType::TurnCompleted
            | EventType::TurnFailed
            | EventType::TurnInterrupted
            | EventType::TurnDispatchFailed
            | EventType::TurnAbandoned => {
                if let Some(turn_id) = &event.turn_id
                    && !self.interaction_terminal_turns.insert(turn_id.clone())
                {
                    return;
                }
                self.interaction_terminal_facts
                    .entry(event.session_id.clone())
                    .and_modify(|at| *at = (*at).max(event.occurred_at))
                    .or_insert(event.occurred_at);
                self.interaction_execution_states.insert(
                    event.session_id.clone(),
                    if event.event_type == EventType::TurnInterrupted {
                        SessionState::Interrupted
                    } else {
                        SessionState::Idle
                    },
                );
                for block in blocks.values_mut() {
                    if block.turn_id == event.turn_id
                        || (block.turn_id.is_none() && block.occurred_at <= event.occurred_at)
                    {
                        block.blocked = false;
                        block.occurred_at = event.occurred_at;
                    }
                }
            }
            EventType::SessionExited | EventType::SessionError => {
                self.interaction_terminal_facts
                    .entry(event.session_id.clone())
                    .and_modify(|at| *at = (*at).max(event.occurred_at))
                    .or_insert(event.occurred_at);
                for block in blocks.values_mut() {
                    block.blocked = false;
                    block.occurred_at = event.occurred_at;
                }
            }
            EventType::SessionCreated
            | EventType::SessionStarting
            | EventType::SessionStarted
            | EventType::SessionResuming
            | EventType::SessionReady
            | EventType::TurnStarted => {
                if event.event_type == EventType::TurnStarted
                    && event
                        .turn_id
                        .as_ref()
                        .is_some_and(|id| self.interaction_terminal_turns.contains(id))
                {
                    return;
                }
                let state = match event.event_type {
                    EventType::SessionCreated => SessionState::Created,
                    EventType::SessionReady => SessionState::Idle,
                    EventType::TurnStarted => SessionState::Busy,
                    _ => SessionState::Starting,
                };
                self.interaction_execution_states
                    .insert(event.session_id.clone(), state);
            }
            _ => {}
        }
    }

    pub(super) fn project_interaction_wait(&mut self, event: &DomainEvent) {
        let Some(session) = self.sessions.get_mut(&event.session_id) else {
            return;
        };
        if session.state.is_terminal() {
            return;
        }
        let blocked = self
            .interaction_blocks
            .get(&event.session_id)
            .is_some_and(|blocks| blocks.values().any(|block| block.blocked));
        let next = if blocked {
            SessionState::WaitingForInteraction
        } else if session.state == SessionState::WaitingForInteraction {
            if self
                .turns
                .values()
                .any(|turn| turn.session_id == event.session_id && turn.state.is_active())
            {
                SessionState::Busy
            } else {
                self.interaction_execution_states
                    .get(&event.session_id)
                    .copied()
                    .unwrap_or(SessionState::Created)
            }
        } else {
            session.state
        };
        if session.state != next || event.event_type.is_interaction_event() {
            session.state = next;
            session.state_version += 1;
        }
    }
}

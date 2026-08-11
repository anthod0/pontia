use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{DomainEvent, EventType, SessionState, TimelineBoundary, TurnState, TurnTopology};
use crate::error::Error;

mod approval;
mod session;
mod turn;

pub const MAX_TURN_INPUT_SUMMARY_CHARS: usize = 1_000;
pub const MAX_TURN_OUTPUT_SUMMARY_CHARS: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProjection {
    pub session_id: String,
    pub client_type: String,
    pub title: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
    pub description: Option<String>,
    pub execution_profile_id: Option<String>,
    pub execution_profile_version: Option<String>,
    pub state: SessionState,
    pub current_turn_id: Option<String>,
    pub state_version: i64,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnProjection {
    pub turn_id: String,
    pub session_id: String,
    pub head_cursor: Option<String>,
    pub tail_cursor: Option<String>,
    pub topology: TurnTopology,
    pub state: TurnState,
    pub state_version: i64,
    pub input_summary: Option<String>,
    pub output_summary: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Default, Clone)]
pub struct ProjectionState {
    sessions: HashMap<String, SessionProjection>,
    turns: HashMap<String, TurnProjection>,
    runtime_bindings: HashMap<String, String>,
}

impl ProjectionState {
    pub fn with_existing(
        sessions: impl IntoIterator<Item = SessionProjection>,
        turns: impl IntoIterator<Item = TurnProjection>,
    ) -> Self {
        Self {
            sessions: sessions
                .into_iter()
                .map(|s| (s.session_id.clone(), s))
                .collect(),
            turns: turns.into_iter().map(|t| (t.turn_id.clone(), t)).collect(),
            runtime_bindings: HashMap::new(),
        }
    }

    pub fn session(&self, session_id: &str) -> Option<&SessionProjection> {
        self.sessions.get(session_id)
    }

    pub fn turn(&self, turn_id: &str) -> Option<&TurnProjection> {
        self.turns.get(turn_id)
    }

    pub fn sessions(&self) -> impl Iterator<Item = &SessionProjection> {
        self.sessions.values()
    }

    pub fn turns(&self) -> impl Iterator<Item = &TurnProjection> {
        self.turns.values()
    }

    pub fn record_runtime_binding(&mut self, session_id: &str, binding: &str) {
        self.runtime_bindings
            .insert(session_id.to_string(), binding.to_string());
    }

    pub fn apply(&mut self, event: &DomainEvent) -> crate::error::Result<()> {
        self.validate_event_shape(event)?;

        if let Some(session) = self.sessions.get(&event.session_id)
            && session.state.is_terminal()
            && !(session.state == SessionState::Exited
                && event.event_type == EventType::SessionResuming)
            && event.event_type != EventType::SessionTitleUpdated
            && event.event_type != EventType::SessionContextUsageUpdated
        {
            if event.topology.is_some() {
                self.apply_topology_to_existing_turn(event)?;
            }
            return Ok(());
        }

        match event.event_type {
            EventType::SessionCreated => self.apply_session(event, SessionState::Created),
            EventType::SessionStarting | EventType::SessionResuming => {
                self.apply_session(event, SessionState::Starting)
            }
            EventType::SessionStarted => self.apply_session(event, SessionState::Starting),
            EventType::SessionReady => self.apply_session(event, SessionState::Idle),
            EventType::SessionExited => {
                self.abandon_active_turn_for_session_exit(event)?;
                self.apply_session(event, SessionState::Exited)?;
                self.clear_approval_interaction(&event.session_id, None)
            }
            EventType::SessionError => {
                self.apply_session(event, SessionState::Error)?;
                self.clear_approval_interaction(&event.session_id, None)
            }
            EventType::SessionTitleUpdated => self.apply_session(
                event,
                self.sessions
                    .get(&event.session_id)
                    .map(|session| session.state)
                    .unwrap_or(SessionState::Created),
            ),
            EventType::SessionMessageUpdated => Ok(()),
            EventType::SessionContextUsageUpdated => self.apply_context_usage(event),
            EventType::TurnCreated | EventType::TurnQueued => {
                self.apply_turn(event, TurnState::Queued)
            }
            EventType::TurnStarted | EventType::TurnOutput | EventType::TurnInterruptRequested => {
                self.apply_turn(event, TurnState::Running)
            }
            EventType::TurnCompleted => self.apply_turn(event, TurnState::Completed),
            EventType::TurnFailed | EventType::TurnDispatchFailed | EventType::TurnAbandoned => {
                self.apply_turn(event, TurnState::Failed)
            }
            EventType::TurnInterrupted => self.apply_turn(event, TurnState::Interrupted),
            EventType::ApprovalRequested => self.apply_approval_requested(event),
            EventType::ApprovalAccepted
            | EventType::ApprovalRejected
            | EventType::ApprovalCancelled => self.apply_approval_final(event),
            EventType::InboxMessageQueued
            | EventType::InboxMessageDispatched
            | EventType::InboxMessageCancelled
            | EventType::InboxMessageSuperseded
            | EventType::InboxMessageFailed
            | EventType::InboxMessageDismissed => Ok(()),
        }
    }

    fn validate_event_shape(&self, event: &DomainEvent) -> crate::error::Result<()> {
        if event.event_type.requires_turn_id() && event.turn_id.is_none() {
            return Err(Error::Domain(format!(
                "event {} requires turn_id",
                event.event_type
            )));
        }
        match (&event.timeline_boundary, event.event_type) {
            (None, _) | (Some(TimelineBoundary::Head { .. }), EventType::TurnStarted) => {}
            (
                Some(TimelineBoundary::Tail { .. }),
                EventType::TurnCompleted
                | EventType::TurnFailed
                | EventType::TurnDispatchFailed
                | EventType::TurnAbandoned
                | EventType::TurnInterrupted,
            ) => {}
            _ => {
                return Err(Error::Domain(format!(
                    "event {} cannot carry its timeline boundary position",
                    event.event_type
                )));
            }
        }
        if event.topology.is_some() && event.event_type != EventType::TurnStarted {
            return Err(Error::Domain(format!(
                "event {} cannot carry Turn topology enrichment",
                event.event_type
            )));
        }
        Ok(())
    }
}

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{DomainEvent, EventType, SessionState, TurnState};
use crate::error::Error;

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
    pub state: TurnState,
    pub state_version: i64,
    pub metadata: Value,
}

#[derive(Debug, Default, Clone)]
pub struct ProjectionState {
    sessions: HashMap<String, SessionProjection>,
    turns: HashMap<String, TurnProjection>,
    runtime_bindings: HashMap<String, String>,
    artifact_index: HashMap<String, (String, Option<String>)>,
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
            artifact_index: HashMap::new(),
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

    pub fn record_artifact_index(
        &mut self,
        artifact_id: &str,
        session_id: &str,
        turn_id: Option<&str>,
    ) {
        self.artifact_index.insert(
            artifact_id.to_string(),
            (session_id.to_string(), turn_id.map(str::to_string)),
        );
    }

    pub fn apply(&mut self, event: &DomainEvent) -> crate::error::Result<()> {
        if event.event_type.requires_turn_id() && event.turn_id.is_none() {
            return Err(Error::Domain(format!(
                "event {} requires turn_id",
                event.event_type
            )));
        }

        if let Some(session) = self.sessions.get(&event.session_id)
            && session.state.is_terminal()
            && !(session.state == SessionState::Exited
                && event.event_type == EventType::SessionResuming)
            && event.event_type != EventType::SessionTitleUpdated
            && event.event_type != EventType::SessionContextUsageUpdated
        {
            return Ok(());
        }

        match event.event_type {
            EventType::SessionCreated => self.apply_session(event, SessionState::Created),
            EventType::SessionStarting | EventType::SessionResuming => {
                self.apply_session(event, SessionState::Starting)
            }
            EventType::SessionStarted => self.apply_session(event, SessionState::Starting),
            EventType::SessionReady => self.apply_session(event, SessionState::Idle),
            EventType::SessionExited => self.apply_session(event, SessionState::Exited),
            EventType::SessionError => self.apply_session(event, SessionState::Error),
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
            EventType::TurnFailed => self.apply_turn(event, TurnState::Failed),
            EventType::TurnInterrupted => self.apply_turn(event, TurnState::Interrupted),
            EventType::TurnCancelled => self.apply_turn(event, TurnState::Cancelled),
            EventType::InboxMessageQueued
            | EventType::InboxMessageDispatched
            | EventType::InboxMessageCancelled
            | EventType::InboxMessageSuperseded
            | EventType::InboxMessageFailed
            | EventType::InboxMessageDismissed => Ok(()),
        }
    }

    fn apply_session(
        &mut self,
        event: &DomainEvent,
        state: SessionState,
    ) -> crate::error::Result<()> {
        let session = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| SessionProjection {
                session_id: event.session_id.clone(),
                client_type: event.client_type.clone(),
                title: None,
                handle: None,
                role: None,
                description: None,
                execution_profile_id: None,
                execution_profile_version: None,
                state: SessionState::Created,
                current_turn_id: None,
                state_version: 0,
                metadata: Value::Object(Default::default()),
            });

        session.state = state;
        if event.event_type == EventType::SessionCreated {
            session.title = event
                .payload
                .get("title")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.handle = event
                .payload
                .get("handle")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.role = event
                .payload
                .get("role")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.description = event
                .payload
                .get("description")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.execution_profile_id = event
                .payload
                .get("execution_profile_id")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            session.execution_profile_version = event
                .payload
                .get("execution_profile_version")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            if let Some(metadata) = event.payload.get("metadata") {
                session.metadata = metadata.clone();
            }
        }
        if event.event_type == EventType::SessionTitleUpdated {
            session.title = event
                .payload
                .get("title")
                .and_then(Value::as_str)
                .map(ToString::to_string);
        }
        if state.is_terminal() {
            session.current_turn_id = None;
        }
        session.state_version += 1;
        Ok(())
    }

    fn apply_context_usage(&mut self, event: &DomainEvent) -> crate::error::Result<()> {
        let Some(session) = self.sessions.get_mut(&event.session_id) else {
            return Ok(());
        };
        let usage = event
            .payload
            .get("context_usage")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                Error::Domain("payload.context_usage must be a JSON object".to_string())
            })?;
        let observed_at = event
            .occurred_at
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|err| Error::Domain(format!("invalid event timestamp: {err}")))?;
        let context_usage = json!({
            "used_tokens": usage.get("used_tokens").cloned().unwrap_or(Value::Null),
            "max_tokens": usage.get("max_tokens").cloned().unwrap_or(Value::Null),
            "remaining_tokens": usage.get("remaining_tokens").cloned().unwrap_or(Value::Null),
            "usage_ratio": usage.get("usage_ratio").cloned().unwrap_or(Value::Null),
            "input_tokens": usage.get("input_tokens").cloned().unwrap_or(Value::Null),
            "output_tokens": usage.get("output_tokens").cloned().unwrap_or(Value::Null),
            "cache_tokens": usage.get("cache_tokens").cloned().unwrap_or(Value::Null),
            "confidence": usage.get("confidence").cloned().unwrap_or_else(|| json!("unknown")),
            "observed_at": observed_at,
        });
        if !session.metadata.is_object() {
            session.metadata = json!({});
        }
        if let Some(metadata) = session.metadata.as_object_mut() {
            metadata.insert("context_usage".to_string(), context_usage);
            if let Some(model) = event.payload.get("model") {
                metadata.insert("model".to_string(), model.clone());
            }
        }
        session.state_version += 1;
        Ok(())
    }

    fn apply_turn(
        &mut self,
        event: &DomainEvent,
        new_state: TurnState,
    ) -> crate::error::Result<()> {
        let turn_id = event.turn_id.as_deref().expect("validated turn_id");

        if let Some(existing) = self.turns.get(turn_id)
            && existing.state.is_terminal()
        {
            return Ok(());
        }

        if new_state.is_active()
            && let Some(session) = self.sessions.get(&event.session_id)
            && let Some(active_turn_id) = &session.current_turn_id
            && active_turn_id != turn_id
        {
            return Err(Error::Domain(format!(
                "session {} already has active turn {}",
                event.session_id, active_turn_id
            )));
        }

        let turn = self
            .turns
            .entry(turn_id.to_string())
            .or_insert_with(|| TurnProjection {
                turn_id: turn_id.to_string(),
                session_id: event.session_id.clone(),
                state: TurnState::Queued,
                state_version: 0,
                metadata: Value::Object(Default::default()),
            });

        turn.state = new_state;
        if event.event_type == EventType::TurnCreated
            && let Some(metadata) = event.payload.get("metadata")
        {
            turn.metadata = metadata.clone();
        }
        turn.state_version += 1;

        let session = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| SessionProjection {
                session_id: event.session_id.clone(),
                client_type: event.client_type.clone(),
                title: None,
                handle: None,
                role: None,
                description: None,
                execution_profile_id: None,
                execution_profile_version: None,
                state: SessionState::Created,
                current_turn_id: None,
                state_version: 0,
                metadata: Value::Object(Default::default()),
            });

        match new_state {
            TurnState::Queued | TurnState::Running => {
                session.current_turn_id = Some(turn_id.to_string());
                if new_state == TurnState::Running {
                    session.state = SessionState::Busy;
                    session.state_version += 1;
                }
            }
            TurnState::Completed | TurnState::Failed | TurnState::Cancelled => {
                if session.current_turn_id.as_deref() == Some(turn_id) {
                    session.current_turn_id = None;
                }
                if session.state == SessionState::Busy {
                    session.state = SessionState::Idle;
                    session.state_version += 1;
                }
            }
            TurnState::Interrupted => {
                if session.current_turn_id.as_deref() == Some(turn_id) {
                    session.current_turn_id = None;
                }
                session.state = SessionState::Interrupted;
                session.state_version += 1;
            }
        }

        Ok(())
    }
}

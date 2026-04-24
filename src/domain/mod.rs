//! Domain model boundary for session / turn state projection.
//!
//! This module is intentionally free of HTTP transport and persistence types.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::{error::Error, time::utc_now};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Starting,
    Idle,
    Busy,
    Interrupted,
    Exited,
    Error,
}

impl SessionState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Exited | Self::Error)
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Created => "created",
            Self::Starting => "starting",
            Self::Idle => "idle",
            Self::Busy => "busy",
            Self::Interrupted => "interrupted",
            Self::Exited => "exited",
            Self::Error => "error",
        })
    }
}

impl std::str::FromStr for SessionState {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "created" => Ok(Self::Created),
            "starting" => Ok(Self::Starting),
            "idle" => Ok(Self::Idle),
            "busy" => Ok(Self::Busy),
            "interrupted" => Ok(Self::Interrupted),
            "exited" => Ok(Self::Exited),
            "error" => Ok(Self::Error),
            _ => Err(Error::Domain(format!("unknown session state: {value}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum TurnState {
    Queued,
    Running,
    Completed,
    Failed,
    Interrupted,
    Cancelled,
}

impl TurnState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Interrupted | Self::Cancelled
        )
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

impl std::fmt::Display for TurnState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Cancelled => "cancelled",
        })
    }
}

impl std::str::FromStr for TurnState {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "interrupted" => Ok(Self::Interrupted),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(Error::Domain(format!("unknown turn state: {value}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    ExternalApi,
    RuntimeManager,
    AgentAdapter,
    AgentClient,
    SystemMonitor,
}

impl std::fmt::Display for EventSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ExternalApi => "external_api",
            Self::RuntimeManager => "runtime_manager",
            Self::AgentAdapter => "agent_adapter",
            Self::AgentClient => "agent_client",
            Self::SystemMonitor => "system_monitor",
        })
    }
}

impl std::str::FromStr for EventSource {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "external_api" => Ok(Self::ExternalApi),
            "runtime_manager" => Ok(Self::RuntimeManager),
            "agent_adapter" => Ok(Self::AgentAdapter),
            "agent_client" => Ok(Self::AgentClient),
            "system_monitor" => Ok(Self::SystemMonitor),
            _ => Err(Error::Domain(format!("unknown event source: {value}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "session.created")]
    SessionCreated,
    #[serde(rename = "session.starting")]
    SessionStarting,
    #[serde(rename = "session.started")]
    SessionStarted,
    #[serde(rename = "session.ready")]
    SessionReady,
    #[serde(rename = "session.exited")]
    SessionExited,
    #[serde(rename = "session.error")]
    SessionError,
    #[serde(rename = "turn.created")]
    TurnCreated,
    #[serde(rename = "turn.queued")]
    TurnQueued,
    #[serde(rename = "turn.started")]
    TurnStarted,
    #[serde(rename = "turn.output")]
    TurnOutput,
    #[serde(rename = "turn.completed")]
    TurnCompleted,
    #[serde(rename = "turn.failed")]
    TurnFailed,
    #[serde(rename = "turn.interrupt_requested")]
    TurnInterruptRequested,
    #[serde(rename = "turn.interrupted")]
    TurnInterrupted,
    #[serde(rename = "turn.cancelled")]
    TurnCancelled,
}

impl EventType {
    pub fn requires_turn_id(self) -> bool {
        matches!(
            self,
            Self::TurnCreated
                | Self::TurnQueued
                | Self::TurnStarted
                | Self::TurnOutput
                | Self::TurnCompleted
                | Self::TurnFailed
                | Self::TurnInterruptRequested
                | Self::TurnInterrupted
                | Self::TurnCancelled
        )
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SessionCreated => "session.created",
            Self::SessionStarting => "session.starting",
            Self::SessionStarted => "session.started",
            Self::SessionReady => "session.ready",
            Self::SessionExited => "session.exited",
            Self::SessionError => "session.error",
            Self::TurnCreated => "turn.created",
            Self::TurnQueued => "turn.queued",
            Self::TurnStarted => "turn.started",
            Self::TurnOutput => "turn.output",
            Self::TurnCompleted => "turn.completed",
            Self::TurnFailed => "turn.failed",
            Self::TurnInterruptRequested => "turn.interrupt_requested",
            Self::TurnInterrupted => "turn.interrupted",
            Self::TurnCancelled => "turn.cancelled",
        })
    }
}

impl std::str::FromStr for EventType {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "session.created" => Ok(Self::SessionCreated),
            "session.starting" => Ok(Self::SessionStarting),
            "session.started" => Ok(Self::SessionStarted),
            "session.ready" => Ok(Self::SessionReady),
            "session.exited" => Ok(Self::SessionExited),
            "session.error" => Ok(Self::SessionError),
            "turn.created" => Ok(Self::TurnCreated),
            "turn.queued" => Ok(Self::TurnQueued),
            "turn.started" => Ok(Self::TurnStarted),
            "turn.output" => Ok(Self::TurnOutput),
            "turn.completed" => Ok(Self::TurnCompleted),
            "turn.failed" => Ok(Self::TurnFailed),
            "turn.interrupt_requested" => Ok(Self::TurnInterruptRequested),
            "turn.interrupted" => Ok(Self::TurnInterrupted),
            "turn.cancelled" => Ok(Self::TurnCancelled),
            _ => Err(Error::Domain(format!("unknown event type: {value}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomainEvent {
    pub event_id: String,
    pub session_id: String,
    pub turn_id: Option<String>,
    pub source: EventSource,
    pub client_type: String,
    pub event_type: EventType,
    pub occurred_at: OffsetDateTime,
    pub seq: Option<i64>,
    pub payload: Value,
}

impl DomainEvent {
    pub fn new(
        event_id: String,
        session_id: String,
        turn_id: Option<String>,
        source: EventSource,
        client_type: String,
        event_type: EventType,
        payload: Value,
    ) -> Self {
        Self {
            event_id,
            session_id,
            turn_id,
            source,
            client_type,
            event_type,
            occurred_at: utc_now(),
            seq: None,
            payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionProjection {
    pub session_id: String,
    pub client_type: String,
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

        if self
            .sessions
            .get(&event.session_id)
            .is_some_and(|session| session.state.is_terminal())
        {
            return Ok(());
        }

        match event.event_type {
            EventType::SessionCreated => self.apply_session(event, SessionState::Created),
            EventType::SessionStarting => self.apply_session(event, SessionState::Starting),
            EventType::SessionStarted | EventType::SessionReady => {
                self.apply_session(event, SessionState::Idle)
            }
            EventType::SessionExited => self.apply_session(event, SessionState::Exited),
            EventType::SessionError => self.apply_session(event, SessionState::Error),
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
                state: SessionState::Created,
                current_turn_id: None,
                state_version: 0,
                metadata: Value::Object(Default::default()),
            });

        session.state = state;
        if state.is_terminal() {
            session.current_turn_id = None;
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
        turn.state_version += 1;

        let session = self
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| SessionProjection {
                session_id: event.session_id.clone(),
                client_type: event.client_type.clone(),
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

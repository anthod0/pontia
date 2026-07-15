use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

use crate::{error::Error, time::utc_now};

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
    #[serde(rename = "session.resuming")]
    SessionResuming,
    #[serde(rename = "session.started")]
    SessionStarted,
    #[serde(rename = "session.ready")]
    SessionReady,
    #[serde(rename = "session.exited")]
    SessionExited,
    #[serde(rename = "session.error")]
    SessionError,
    #[serde(rename = "session.title_updated")]
    SessionTitleUpdated,
    #[serde(rename = "session.message_updated")]
    SessionMessageUpdated,
    #[serde(rename = "session.context_usage_updated")]
    SessionContextUsageUpdated,
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
    #[serde(rename = "inbox.message_queued")]
    InboxMessageQueued,
    #[serde(rename = "inbox.message_dispatched")]
    InboxMessageDispatched,
    #[serde(rename = "inbox.message_cancelled")]
    InboxMessageCancelled,
    #[serde(rename = "inbox.message_superseded")]
    InboxMessageSuperseded,
    #[serde(rename = "inbox.message_failed")]
    InboxMessageFailed,
    #[serde(rename = "inbox.message_dismissed")]
    InboxMessageDismissed,
}

impl EventType {
    pub fn is_turn_event(self) -> bool {
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

    pub fn requires_turn_id(self) -> bool {
        self.is_turn_event()
    }
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SessionCreated => "session.created",
            Self::SessionStarting => "session.starting",
            Self::SessionResuming => "session.resuming",
            Self::SessionStarted => "session.started",
            Self::SessionReady => "session.ready",
            Self::SessionExited => "session.exited",
            Self::SessionError => "session.error",
            Self::SessionTitleUpdated => "session.title_updated",
            Self::SessionMessageUpdated => "session.message_updated",
            Self::SessionContextUsageUpdated => "session.context_usage_updated",
            Self::TurnCreated => "turn.created",
            Self::TurnQueued => "turn.queued",
            Self::TurnStarted => "turn.started",
            Self::TurnOutput => "turn.output",
            Self::TurnCompleted => "turn.completed",
            Self::TurnFailed => "turn.failed",
            Self::TurnInterruptRequested => "turn.interrupt_requested",
            Self::TurnInterrupted => "turn.interrupted",
            Self::TurnCancelled => "turn.cancelled",
            Self::InboxMessageQueued => "inbox.message_queued",
            Self::InboxMessageDispatched => "inbox.message_dispatched",
            Self::InboxMessageCancelled => "inbox.message_cancelled",
            Self::InboxMessageSuperseded => "inbox.message_superseded",
            Self::InboxMessageFailed => "inbox.message_failed",
            Self::InboxMessageDismissed => "inbox.message_dismissed",
        })
    }
}

impl std::str::FromStr for EventType {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "session.created" => Ok(Self::SessionCreated),
            "session.starting" => Ok(Self::SessionStarting),
            "session.resuming" => Ok(Self::SessionResuming),
            "session.started" => Ok(Self::SessionStarted),
            "session.ready" => Ok(Self::SessionReady),
            "session.exited" => Ok(Self::SessionExited),
            "session.error" => Ok(Self::SessionError),
            "session.title_updated" => Ok(Self::SessionTitleUpdated),
            "session.message_updated" => Ok(Self::SessionMessageUpdated),
            "session.context_usage_updated" => Ok(Self::SessionContextUsageUpdated),
            "turn.created" => Ok(Self::TurnCreated),
            "turn.queued" => Ok(Self::TurnQueued),
            "turn.started" => Ok(Self::TurnStarted),
            "turn.output" => Ok(Self::TurnOutput),
            "turn.completed" => Ok(Self::TurnCompleted),
            "turn.failed" => Ok(Self::TurnFailed),
            "turn.interrupt_requested" => Ok(Self::TurnInterruptRequested),
            "turn.interrupted" => Ok(Self::TurnInterrupted),
            "turn.cancelled" => Ok(Self::TurnCancelled),
            "inbox.message_queued" => Ok(Self::InboxMessageQueued),
            "inbox.message_dispatched" => Ok(Self::InboxMessageDispatched),
            "inbox.message_cancelled" => Ok(Self::InboxMessageCancelled),
            "inbox.message_superseded" => Ok(Self::InboxMessageSuperseded),
            "inbox.message_failed" => Ok(Self::InboxMessageFailed),
            "inbox.message_dismissed" => Ok(Self::InboxMessageDismissed),
            _ => Err(Error::Domain(format!("unknown event type: {value}"))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReportedEvent {
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

impl ReportedEvent {
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
#[serde(tag = "position", rename_all = "snake_case")]
pub enum TimelineBoundary {
    Head { cursor: String },
    Tail { cursor: String },
}

impl TimelineBoundary {
    pub fn head(cursor: impl Into<String>) -> Self {
        Self::Head {
            cursor: cursor.into(),
        }
    }

    pub fn tail(cursor: impl Into<String>) -> Self {
        Self::Tail {
            cursor: cursor.into(),
        }
    }

    pub fn cursor(&self) -> &str {
        match self {
            Self::Head { cursor } | Self::Tail { cursor } => cursor,
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
    pub turn_index: Option<i64>,
    pub timeline_boundary: Option<TimelineBoundary>,
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
            turn_index: None,
            timeline_boundary: None,
        }
    }

    pub fn with_turn_index(mut self, turn_index: i64) -> Self {
        self.turn_index = Some(turn_index);
        self
    }

    pub fn with_timeline_boundary(mut self, boundary: TimelineBoundary) -> Self {
        self.timeline_boundary = Some(boundary);
        self
    }
}

impl From<ReportedEvent> for DomainEvent {
    fn from(event: ReportedEvent) -> Self {
        Self {
            event_id: event.event_id,
            session_id: event.session_id,
            turn_id: event.turn_id,
            source: event.source,
            client_type: event.client_type,
            event_type: event.event_type,
            occurred_at: event.occurred_at,
            seq: event.seq,
            payload: event.payload,
            turn_index: None,
            timeline_boundary: None,
        }
    }
}

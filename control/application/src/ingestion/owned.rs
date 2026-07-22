use serde_json::Value;

use pontia_core::{
    domain::{EventSource, EventType, ReportedEvent},
    ids::new_event_id,
};

/// A fact whose authority belongs to the Pontia control plane.
///
/// This type deliberately excludes agent-client lifecycle observations such as
/// `turn.started`, `turn.output`, `turn.completed`, and `turn.failed`. Those
/// facts must enter through the reported-fact ingestion path.
///
/// ```compile_fail
/// use pontia_application::PontiaEventType;
///
/// let _ = PontiaEventType::TurnStarted;
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PontiaEventType {
    SessionCreated,
    SessionStarting,
    SessionResuming,
    SessionStarted,
    SessionReady,
    SessionExited,
    SessionError,
    SessionTitleUpdated,
    TurnCreated,
    TurnQueued,
    TurnDispatchFailed,
    TurnAbandoned,
    TurnInterruptRequested,
    TurnInterrupted,
    TurnCancelled,
    InboxMessageQueued,
    InboxMessageDispatched,
    InboxMessageCancelled,
    InboxMessageSuperseded,
    InboxMessageFailed,
    InboxMessageDismissed,
}

impl From<PontiaEventType> for EventType {
    fn from(value: PontiaEventType) -> Self {
        match value {
            PontiaEventType::SessionCreated => Self::SessionCreated,
            PontiaEventType::SessionStarting => Self::SessionStarting,
            PontiaEventType::SessionResuming => Self::SessionResuming,
            PontiaEventType::SessionStarted => Self::SessionStarted,
            PontiaEventType::SessionReady => Self::SessionReady,
            PontiaEventType::SessionExited => Self::SessionExited,
            PontiaEventType::SessionError => Self::SessionError,
            PontiaEventType::SessionTitleUpdated => Self::SessionTitleUpdated,
            PontiaEventType::TurnCreated => Self::TurnCreated,
            PontiaEventType::TurnQueued => Self::TurnQueued,
            PontiaEventType::TurnDispatchFailed => Self::TurnDispatchFailed,
            PontiaEventType::TurnAbandoned => Self::TurnAbandoned,
            PontiaEventType::TurnInterruptRequested => Self::TurnInterruptRequested,
            PontiaEventType::TurnInterrupted => Self::TurnInterrupted,
            PontiaEventType::TurnCancelled => Self::TurnCancelled,
            PontiaEventType::InboxMessageQueued => Self::InboxMessageQueued,
            PontiaEventType::InboxMessageDispatched => Self::InboxMessageDispatched,
            PontiaEventType::InboxMessageCancelled => Self::InboxMessageCancelled,
            PontiaEventType::InboxMessageSuperseded => Self::InboxMessageSuperseded,
            PontiaEventType::InboxMessageFailed => Self::InboxMessageFailed,
            PontiaEventType::InboxMessageDismissed => Self::InboxMessageDismissed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PontiaEventSource {
    ExternalApi,
    RuntimeManager,
    SystemMonitor,
}

impl From<PontiaEventSource> for EventSource {
    fn from(value: PontiaEventSource) -> Self {
        match value {
            PontiaEventSource::ExternalApi => Self::ExternalApi,
            PontiaEventSource::RuntimeManager => Self::RuntimeManager,
            PontiaEventSource::SystemMonitor => Self::SystemMonitor,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PontiaEvent(ReportedEvent);

impl PontiaEvent {
    pub fn new(
        session_id: impl Into<String>,
        turn_id: Option<String>,
        source: PontiaEventSource,
        client_type: impl Into<String>,
        event_type: PontiaEventType,
        payload: Value,
    ) -> Self {
        Self(ReportedEvent::new(
            new_event_id().to_string(),
            session_id.into(),
            turn_id,
            source.into(),
            client_type.into(),
            event_type.into(),
            payload,
        ))
    }

    pub(crate) fn into_reported_event(self) -> ReportedEvent {
        self.0
    }
}

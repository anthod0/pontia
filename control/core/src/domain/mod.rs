//! Domain model boundary for session / turn state projection.
//!
//! This module is intentionally free of HTTP transport and persistence types.

mod event;
mod projection;
mod state;

pub use event::{DomainEvent, EventSource, EventType, ReportedEvent, TimelineBoundary};
pub use projection::{
    MAX_TURN_INPUT_SUMMARY_CHARS, MAX_TURN_OUTPUT_SUMMARY_CHARS, ProjectionState,
    SessionProjection, TurnProjection,
};
pub use state::{SessionState, TurnState};

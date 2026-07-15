//! Domain model boundary for session / turn state projection.
//!
//! This module is intentionally free of HTTP transport and persistence types.

mod event;
mod projection;
mod state;

pub use event::{DomainEvent, EventSource, EventType, ReportedEvent};
pub use projection::{ProjectionState, SessionProjection, TurnProjection};
pub use state::{SessionState, TurnState};

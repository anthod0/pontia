mod observations;
pub use observations::{NativeTurnObservation, NativeTurnService};
mod identity;
pub(crate) use identity::native_turn_identity;
mod commands;
mod interrupt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputIntent {
    Start,
    Steer { turn_id: String },
}

pub use commands::TurnCommandService;

#[cfg(test)]
mod tests;

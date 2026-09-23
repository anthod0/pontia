pub mod claim;
pub mod commands;
mod context;
mod interrupt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputIntent {
    Start,
    Steer { turn_id: String },
}

pub use claim::{CurrentTurnClaimRequest, CurrentTurnClaimService};
pub use commands::TurnCommandService;
pub(crate) use context::store_client_current_turn_context;

#[cfg(test)]
mod tests;

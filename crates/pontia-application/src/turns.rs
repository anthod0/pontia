pub mod commands;
mod interrupt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputIntent {
    Start,
    Steer { turn_id: String },
}

pub use commands::TurnCommandService;

#[cfg(test)]
mod tests;

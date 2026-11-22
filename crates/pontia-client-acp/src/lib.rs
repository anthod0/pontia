mod adapter;
mod agents;
mod capabilities;
pub mod configuration;
mod connection;
pub use agents::{AcpAgentDefinition, AcpAgents, AcpLaunch};
mod history;
pub mod identity;
mod instructions;
mod interactions;
mod live_output;
mod models;
mod peer;
mod spec;
mod steering;
mod transport;

pub use adapter::{registration, registration_with_relay};
pub use spec::{ACP_MAX_CAPABILITIES, SPEC};

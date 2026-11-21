mod adapter;
pub mod configuration;
mod connection;
pub use configuration::{AcpAgentConfig, AcpConfig};
mod history;
pub mod identity;
mod instructions;
mod interactions;
mod live_output;
mod models;
mod peer;
pub mod registry;
mod spec;
mod steering;
mod transport;

pub use adapter::{
    registration, registration_from_sources, registration_with_registry, registration_with_relay,
};
pub use spec::{ACP_MAX_CAPABILITIES, SPEC};

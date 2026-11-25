mod adapter;
mod agents;
mod antigravity;
mod auth;
mod capabilities;
mod compaction;
pub mod configuration;
mod connection;
pub use agents::{
    AcpAgentDefinition, AcpAgents, AcpLaunch, CompactionProtocol, InterjectionProtocol,
};
mod history;
pub mod identity;
mod instructions;
mod interactions;
mod kiro;
mod live_output;
mod models;
mod observations;
mod peer;
pub mod relay;
mod restrictions;
pub use restrictions::{AgentProfile, RestrictedExecution};
mod spec;
mod stdio;
mod steering;
mod transport;

pub use adapter::{registration, registration_with_relay};
pub use spec::{ACP_MAX_CAPABILITIES, SPEC};

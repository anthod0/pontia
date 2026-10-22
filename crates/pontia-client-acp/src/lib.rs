mod adapter;
pub mod configuration;
pub use configuration::{AcpAgentConfig, AcpConfig};
mod history;
pub mod identity;
mod interactions;
mod live_output;
mod peer;
pub mod registry;
mod spec;
mod transport;

pub use adapter::{registration, registration_from_sources, registration_with_registry};
pub use spec::{ACP_MAX_CAPABILITIES, SPEC};

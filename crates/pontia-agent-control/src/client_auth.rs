//! Interactive Agent Client login sessions held in pontiad memory.
//!
//! A login session is not a durable Agent Session. Credentials stay in each
//! client's native directory; this module only tracks short-lived progress.

mod manager;
mod sanitize;
#[cfg(test)]
mod tests;

pub use manager::{ClientAuthProbe, ClientAuthProbeEntry, ClientAuthService};
pub use sanitize::{sanitize_output, strip_ansi};

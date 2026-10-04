//! Browser/device E2E protocol. This crate has no networking or business state.
pub mod bhttp;
mod capability;
mod handshake;
mod records;
mod sessions;
#[cfg(test)]
mod vectors;
#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests;

pub use capability::{CAPABILITY_BYTES, Capability};
pub use handshake::{BrowserHandshake, BrowserIdentity, BrowserSession, DeviceIdentity};
pub use records::{Direction, MAX_RECORD_PLAINTEXT, RecordDecoder, RecordEncoder, RequestContext};
pub use sessions::{
    DeviceSessions, IDLE_SECONDS, MAX_HANDSHAKES_PER_SECOND, MAX_REQUESTS, MAX_SESSIONS,
    MAX_STREAMS, StreamLease,
};

pub type Id = [u8; 32];
pub type DeviceId = [u8; 16];
pub const VERSION: u8 = 1;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    #[error("invalid E2E protocol input")]
    Protocol,
    #[error("E2E authentication failed")]
    Authentication,
    #[error("handshake authorization expired or not yet valid")]
    AuthorizationWindow,
    #[error("unknown E2E session")]
    UnknownSession,
    #[error("request replay")]
    Replay,
    #[error("E2E resource limit reached")]
    Capacity,
    #[error("truncated E2E stream")]
    Truncated,
    #[error("E2E handle is closed")]
    Closed,
    #[error("secure randomness unavailable")]
    Randomness,
}

pub type Result<T> = std::result::Result<T, Error>;

fn random<const N: usize>() -> Result<[u8; N]> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| Error::Randomness)?;
    Ok(bytes)
}

// Labels are fixed; all variable fields have a big-endian u32 length prefix.
fn context(label: &[u8], fields: &[&[u8]]) -> Vec<u8> {
    let mut output = b"pontia-e2e-v1\0".to_vec();
    for field in std::iter::once(&label).chain(fields.iter()) {
        output.extend_from_slice(&(field.len() as u32).to_be_bytes());
        output.extend_from_slice(field);
    }
    output
}

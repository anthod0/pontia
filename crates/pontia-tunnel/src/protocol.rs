use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result};

pub const VERSION: u16 = 1;
pub const MAX_MESSAGE_BYTES: usize = 4096;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Message {
    Challenge { version: u16, nonce: [u8; 32] },
    Authenticate { device_id: Uuid, signature: String },
    Authenticated { device_id: Uuid },
    Ping { nonce: [u8; 32] },
    Pong { nonce: [u8; 32] },
}

pub fn nonce() -> Result<[u8; 32]> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes)?;
    Ok(bytes)
}

// Fixed-width fields and a versioned domain separator make the signed identity unambiguous.
pub fn signing_payload(device_id: Uuid, nonce: &[u8; 32]) -> Vec<u8> {
    let mut bytes = b"pontia/tunnel/device-auth/v1\0".to_vec();
    bytes.extend_from_slice(device_id.as_bytes());
    bytes.extend_from_slice(nonce);
    bytes
}

pub fn verify(
    public_key: &[u8; 32],
    device_id: Uuid,
    nonce: &[u8; 32],
    signature: &str,
) -> Result<()> {
    let invalid = || Error::Protocol("invalid device signature");
    let key = VerifyingKey::from_bytes(public_key).map_err(|_| invalid())?;
    let signature = STANDARD.decode(signature).map_err(|_| invalid())?;
    let signature = Signature::from_slice(&signature).map_err(|_| invalid())?;
    key.verify_strict(&signing_payload(device_id, nonce), &signature)
        .map_err(|_| invalid())
}

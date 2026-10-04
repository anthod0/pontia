use ed25519_dalek::{Signature, VerifyingKey};

use crate::{DeviceId, Error, Id, Result, VERSION};

pub const CAPABILITY_BYTES: usize = 1 + 16 + 8 + 32 + 32 + 8 + 8 + 64;
const SIGNED_BYTES: usize = CAPABILITY_BYTES - 64;
const LABEL: &[u8] = b"pontia-e2e-capability-v1\0";

/// Fixed-width signed capability. Times are unsigned Unix seconds.
#[derive(Clone)]
pub struct Capability {
    pub device_id: DeviceId,
    pub key_version: u64,
    pub browser_public_key: [u8; 32],
    pub authorization_id: Id,
    pub issued_at: u64,
    pub redeem_before: u64,
    signature: [u8; 64],
}

impl Capability {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() != CAPABILITY_BYTES || bytes[0] != VERSION {
            return Err(Error::Protocol);
        }
        Ok(Self {
            device_id: bytes[1..17].try_into().unwrap(),
            key_version: u64::from_be_bytes(bytes[17..25].try_into().unwrap()),
            browser_public_key: bytes[25..57].try_into().unwrap(),
            authorization_id: bytes[57..89].try_into().unwrap(),
            issued_at: u64::from_be_bytes(bytes[89..97].try_into().unwrap()),
            redeem_before: u64::from_be_bytes(bytes[97..105].try_into().unwrap()),
            signature: bytes[105..].try_into().unwrap(),
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(CAPABILITY_BYTES);
        bytes.push(VERSION);
        bytes.extend_from_slice(&self.device_id);
        bytes.extend_from_slice(&self.key_version.to_be_bytes());
        bytes.extend_from_slice(&self.browser_public_key);
        bytes.extend_from_slice(&self.authorization_id);
        bytes.extend_from_slice(&self.issued_at.to_be_bytes());
        bytes.extend_from_slice(&self.redeem_before.to_be_bytes());
        bytes.extend_from_slice(&self.signature);
        bytes
    }

    /// Canonical signing representation used by the Cloud signer.
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = LABEL.to_vec();
        bytes.extend_from_slice(&self.encode()[..SIGNED_BYTES]);
        bytes
    }

    pub fn verify(&self, trusted_key: &VerifyingKey, now: u64) -> Result<()> {
        trusted_key
            .verify_strict(
                &self.signing_bytes(),
                &Signature::from_bytes(&self.signature),
            )
            .map_err(|_| Error::Authentication)?;
        if self.key_version == 0
            || self.issued_at.checked_add(60) != Some(self.redeem_before)
            || now < self.issued_at
            || now >= self.redeem_before
        {
            return Err(Error::AuthorizationWindow);
        }
        Ok(())
    }
}

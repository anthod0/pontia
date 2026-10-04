use std::{fs, path::Path};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use ed25519_dalek::VerifyingKey;
use pontia_e2e::{DeviceIdentity, DeviceSessions};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredIdentity {
    device_id: String,
    key_version: u64,
    private_key: String,
    capability_verification_key: String,
}

pub fn load(home: &Path, expected_device_id: Uuid) -> Result<DeviceSessions, String> {
    let path = home.join("e2e-identity.json");
    let bytes = fs::read(&path).map_err(|error| {
        format!(
            "remote E2E identity is unavailable at {}: {error}; run `pontia remote enable` or `pontia remote rotate-key`",
            path.display()
        )
    })?;
    let stored: StoredIdentity = serde_json::from_slice(&bytes)
        .map_err(|_| format!("remote E2E identity is invalid at {}", path.display()))?;
    if stored.device_id != expected_device_id.to_string() || stored.key_version == 0 {
        return Err(format!(
            "remote E2E identity does not match the configured device at {}",
            path.display()
        ));
    }
    let private = URL_SAFE_NO_PAD
        .decode(stored.private_key)
        .map_err(|_| format!("remote E2E private key is invalid at {}", path.display()))?;
    let verification: [u8; 32] = URL_SAFE_NO_PAD
        .decode(stored.capability_verification_key)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| {
            format!(
                "capability verification key is invalid at {}",
                path.display()
            )
        })?;
    let identity = DeviceIdentity::from_private_bytes(
        *expected_device_id.as_bytes(),
        stored.key_version,
        &private,
    )
    .map_err(|_| format!("remote E2E private key is invalid at {}", path.display()))?;
    let trusted = VerifyingKey::from_bytes(&verification).map_err(|_| {
        format!(
            "capability verification key is invalid at {}",
            path.display()
        )
    })?;
    Ok(DeviceSessions::new(identity, trusted))
}

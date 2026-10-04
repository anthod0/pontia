use ed25519_dalek::{Signer, SigningKey};

/// Encode/sign the fixed-width external contract, independently of the core decoder.
pub fn capability(
    signing: &SigningKey,
    device: [u8; 16],
    version: u64,
    browser: [u8; 32],
    issued: u64,
) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&device);
    bytes.extend_from_slice(&version.to_be_bytes());
    bytes.extend_from_slice(&browser);
    bytes.extend_from_slice(&[4; 32]);
    bytes.extend_from_slice(&issued.to_be_bytes());
    bytes.extend_from_slice(&(issued + 60).to_be_bytes());
    let mut signed = b"pontia-e2e-capability-v1\0".to_vec();
    signed.extend_from_slice(&bytes);
    bytes.extend_from_slice(&signing.sign(&signed).to_bytes());
    bytes
}

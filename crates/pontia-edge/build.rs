fn main() {
    println!("cargo:rerun-if-env-changed=E2E_CAPABILITY_VERIFICATION_KEY");
    if let Ok(key) = std::env::var("E2E_CAPABILITY_VERIFICATION_KEY") {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        assert!(
            key.len() == 43
                && key.bytes().all(|byte| alphabet.contains(&byte))
                && alphabet
                    .iter()
                    .position(|byte| Some(byte) == key.as_bytes().last())
                    .is_some_and(|index| index % 4 == 0),
            "E2E_CAPABILITY_VERIFICATION_KEY must be an unpadded base64url 32-byte Ed25519 public key"
        );
    }
}

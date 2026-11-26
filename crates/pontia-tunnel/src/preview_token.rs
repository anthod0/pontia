use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, get_current_timestamp};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VerifiedPreviewToken {
    pub device: Uuid,
    pub port: u16,
    pub remaining_secs: u64,
}

#[derive(Clone)]
pub struct PreviewTokenVerifier {
    key: DecodingKey,
    validation: Validation,
}

#[derive(Deserialize)]
struct Claims {
    #[serde(rename = "aud")]
    _edge: Uuid,
    did: Uuid,
    sub: Uuid,
    port: u16,
    purpose: String,
    iat: u64,
    exp: u64,
}

impl PreviewTokenVerifier {
    pub fn new(public_key: &[u8], edge: Option<Uuid>) -> Self {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.leeway = 0;
        if let Some(edge) = edge {
            validation.set_audience(&[edge]);
        } else {
            // Device identities pin the Cloud key and device ID, independent of the routing edge.
            validation.validate_aud = false;
        }
        Self {
            key: DecodingKey::from_ed_der(public_key),
            validation,
        }
    }

    pub fn verify(&self, token: &str, device: Uuid, port: u16) -> Option<u64> {
        let verified = self.verify_any(token)?;
        (verified.device == device && verified.port == port).then_some(verified.remaining_secs)
    }

    pub fn verify_any(&self, token: &str) -> Option<VerifiedPreviewToken> {
        let signed = decode::<Claims>(token, &self.key, &self.validation).ok()?;
        let claims = signed.claims;
        let now = get_current_timestamp();
        (signed.header.typ.as_deref() == Some("pontia-preview+jwt")
            && claims.purpose == "localhost_preview"
            && claims.did.get_version_num() == 7
            && pontia_tunnel_port_is_safe(claims.port)
            && !claims.sub.is_nil()
            && claims.iat <= now
            && now < claims.exp
            && claims.exp - claims.iat <= 4 * 3600)
            .then(|| VerifiedPreviewToken {
                device: claims.did,
                port: claims.port,
                remaining_secs: claims.exp - now,
            })
    }
}

fn pontia_tunnel_port_is_safe(port: u16) -> bool {
    crate::endpoint::is_safe_port(port)
}

#[cfg(test)]
mod tests {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::{Value, json};

    use super::*;

    fn token(typ: &str, claims: &Value, key: &SigningKey) -> String {
        let header =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&json!({"alg":"EdDSA", "typ":typ})).unwrap());
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(claims).unwrap());
        let input = format!("{header}.{payload}");
        let signature = key.sign(input.as_bytes());
        format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature.to_bytes()))
    }

    fn device_id() -> Uuid {
        Uuid::parse_str("0199791c-6600-7000-8000-000000000001").unwrap()
    }

    #[test]
    fn verifies_and_extracts_a_safe_preview_target() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let edge = Uuid::new_v4();
        let device = device_id();
        let now = get_current_timestamp();
        let claims = json!({
            "aud": edge,
            "did": device,
            "sub": Uuid::new_v4(),
            "port": 5173,
            "purpose": "localhost_preview",
            "iat": now,
            "exp": now + 3600,
        });
        let verifier = PreviewTokenVerifier::new(key.verifying_key().as_bytes(), Some(edge));

        let verified = verifier
            .verify_any(&token("pontia-preview+jwt", &claims, &key))
            .unwrap();

        assert_eq!(verified.device, device);
        assert_eq!(verified.port, 5173);
        assert!((1..=3600).contains(&verified.remaining_secs));
    }

    #[test]
    fn rejects_tokens_for_unsafe_ports_or_another_edge() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let edge = Uuid::new_v4();
        let now = get_current_timestamp();
        let verifier = PreviewTokenVerifier::new(key.verifying_key().as_bytes(), Some(edge));
        for (audience, port) in [(edge, 22), (Uuid::new_v4(), 5173)] {
            let claims = json!({
                "aud": audience,
                "did": device_id(),
                "sub": Uuid::new_v4(),
                "port": port,
                "purpose": "localhost_preview",
                "iat": now,
                "exp": now + 3600,
            });
            assert!(
                verifier
                    .verify_any(&token("pontia-preview+jwt", &claims, &key))
                    .is_none()
            );
        }
    }

    #[test]
    fn rejects_invalid_identity_purpose_type_and_lifetime_boundaries() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let edge = Uuid::new_v4();
        let now = get_current_timestamp();
        let verifier = PreviewTokenVerifier::new(key.verifying_key().as_bytes(), Some(edge));
        let valid = json!({
            "aud": edge,
            "did": device_id(),
            "sub": Uuid::new_v4(),
            "port": 5173,
            "purpose": "localhost_preview",
            "iat": now,
            "exp": now + 4 * 3600,
        });
        let mut invalid = Vec::new();
        invalid.push(("JWT", valid.clone()));
        for (field, value) in [
            ("purpose", json!("other")),
            ("did", json!(Uuid::new_v4())),
            ("sub", json!(Uuid::nil())),
            ("iat", json!(now + 1)),
            ("exp", json!(now + 4 * 3600 + 1)),
        ] {
            let mut claims = valid.clone();
            claims[field] = value;
            invalid.push(("pontia-preview+jwt", claims));
        }
        let mut expired = valid;
        expired["iat"] = json!(now - 2);
        expired["exp"] = json!(now - 1);
        invalid.push(("pontia-preview+jwt", expired));

        for (typ, claims) in invalid {
            assert!(verifier.verify_any(&token(typ, &claims, &key)).is_none());
        }
    }
}

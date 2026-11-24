use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, get_current_timestamp};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Clone)]
pub struct AccessTokenVerifier {
    key: DecodingKey,
    validation: Validation,
}

#[derive(Deserialize)]
struct Claims {
    device_id: Uuid,
    user_id: Uuid,
    iat: u64,
    exp: u64,
}

impl AccessTokenVerifier {
    pub fn new(edge_id: Uuid, public_key: &str) -> Result<Self> {
        let bytes = URL_SAFE_NO_PAD
            .decode(public_key)
            .context("invalid Edge access verification key")?;
        anyhow::ensure!(
            bytes.len() == 32,
            "expected a raw 32-byte Ed25519 public key"
        );
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.set_audience(&[edge_id]);
        validation.set_required_spec_claims(&["aud", "exp"]);
        validation.leeway = 0;
        Ok(Self {
            key: DecodingKey::from_ed_der(&bytes),
            validation,
        })
    }

    pub(crate) fn authenticate(&self, token: &str) -> Option<Uuid> {
        let claims = decode::<Claims>(token, &self.key, &self.validation)
            .ok()?
            .claims;
        let now = get_current_timestamp();
        (claims.iat <= now
            && now < claims.exp
            && claims.exp - claims.iat <= 4 * 3600
            && claims.device_id.get_version_num() == 7
            && !claims.user_id.is_nil())
        .then_some(claims.device_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_an_unpadded_base64url_raw_ed25519_key() {
        let edge = Uuid::now_v7();
        let key = URL_SAFE_NO_PAD.encode([3; 32]);
        assert!(AccessTokenVerifier::new(edge, &key).is_ok());
        for invalid in [
            "",
            "not a key",
            &format!("{key}="),
            &URL_SAFE_NO_PAD.encode([3; 31]),
        ] {
            assert!(AccessTokenVerifier::new(edge, invalid).is_err());
        }
    }
}

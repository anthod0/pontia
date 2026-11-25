use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, get_current_timestamp};
use serde::{Deserialize, de::DeserializeOwned};
use uuid::Uuid;

#[derive(Clone)]
pub struct AccessTokenVerifier {
    pub(crate) preview: pontia_tunnel::PreviewTokenVerifier,
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
    pub fn embedded(edge_id: Uuid) -> Result<Self> {
        let public_key = option_env!("E2E_CAPABILITY_VERIFICATION_KEY").context(
            "this build has no Edge access verification key; use an official Pontia release",
        )?;
        Self::new(edge_id, public_key)
    }

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
            preview: pontia_tunnel::PreviewTokenVerifier::new(&bytes, Some(edge_id)),
            key: DecodingKey::from_ed_der(&bytes),
            validation,
        })
    }

    fn claims<T: DeserializeOwned>(&self, token: &str, typ: &str) -> Option<T> {
        let signed = decode::<T>(token, &self.key, &self.validation).ok()?;
        (signed.header.typ.as_deref() == Some(typ)).then_some(signed.claims)
    }

    pub(crate) fn update_nonce(&self, token: &str) -> Option<Uuid> {
        #[derive(Deserialize)]
        struct Update {
            #[serde(rename = "aud")]
            _audience: Uuid,
            jti: Uuid,
            iat: u64,
            exp: u64,
        }
        let claims: Update = self.claims(token, "pontia-edge-update+jwt")?;
        remaining_lifetime(claims.iat, claims.exp)?;
        (claims.exp - claims.iat <= 60).then_some(claims.jti)
    }

    /// Returns the nonce of a fresh Cloud ownership challenge for this edge endpoint.
    pub(crate) fn ownership_nonce(&self, token: &str, hostname: &str, port: u16) -> Option<String> {
        #[derive(Deserialize)]
        struct Ownership {
            #[serde(rename = "aud")]
            _audience: Uuid,
            jti: String,
            host: String,
            port: u16,
            iat: u64,
            exp: u64,
        }
        let claims: Ownership = self.claims(token, "pontia-edge-ownership+jwt")?;
        remaining_lifetime(claims.iat, claims.exp)?;
        let nonce = URL_SAFE_NO_PAD.decode(&claims.jti).ok()?;
        (claims.exp - claims.iat <= 60
            && claims.host == hostname
            && claims.port == port
            && claims.jti.len() == 43
            && nonce.len() == 32
            && URL_SAFE_NO_PAD.encode(&nonce) == claims.jti)
            .then_some(claims.jti)
    }

    pub(crate) fn authenticate(&self, token: &str) -> Option<Uuid> {
        let claims: Claims = self.claims(token, "JWT")?;
        remaining_lifetime(claims.iat, claims.exp)?;
        (claims.device_id.get_version_num() == 7 && !claims.user_id.is_nil())
            .then_some(claims.device_id)
    }
}

fn remaining_lifetime(iat: u64, exp: u64) -> Option<u64> {
    let now = get_current_timestamp();
    (iat <= now && now < exp && exp - iat <= 4 * 3600).then(|| exp - now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_verifier_requires_a_build_time_key() {
        let result = AccessTokenVerifier::embedded(Uuid::now_v7());
        if option_env!("E2E_CAPABILITY_VERIFICATION_KEY").is_some() {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }

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

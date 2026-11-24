use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, get_current_timestamp};
use serde::{Deserialize, de::DeserializeOwned};
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
            key: DecodingKey::from_ed_der(&bytes),
            validation,
        })
    }

    fn claims<T: DeserializeOwned>(&self, token: &str, typ: &str) -> Option<T> {
        let signed = decode::<T>(token, &self.key, &self.validation).ok()?;
        (signed.header.typ.as_deref() == Some(typ)).then_some(signed.claims)
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

pub(crate) struct PreviewTokenVerifier<'a>(pub &'a AccessTokenVerifier);

#[derive(Deserialize)]
struct PreviewClaims {
    // Require a single audience. Validation compares it to this edge ID.
    #[serde(rename = "aud")]
    _audience: Uuid,
    did: Uuid,
    sub: Uuid,
    port: u16,
    purpose: String,
    iat: u64,
    exp: u64,
}

impl PreviewTokenVerifier<'_> {
    pub fn verify(&self, token: &str, device: Uuid, port: u16) -> Option<u64> {
        let claims: PreviewClaims = self.0.claims(token, "pontia-preview+jwt")?;
        if claims.purpose != "localhost_preview"
            || claims.did != device
            || device.get_version_num() != 7
            || claims.port != port
            || port == 0
            || claims.sub.is_nil()
        {
            return None;
        }
        remaining_lifetime(claims.iat, claims.exp)
    }
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

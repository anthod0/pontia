use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, get_current_timestamp};
use serde::Deserialize;
use uuid::Uuid;

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
        let signed = decode::<Claims>(token, &self.key, &self.validation).ok()?;
        let claims = signed.claims;
        let now = get_current_timestamp();
        (signed.header.typ.as_deref() == Some("pontia-preview+jwt")
            && claims.purpose == "localhost_preview"
            && claims.did == device
            && device.get_version_num() == 7
            && claims.port == port
            && port != 0
            && !claims.sub.is_nil()
            && claims.iat <= now
            && now < claims.exp
            && claims.exp - claims.iat <= 4 * 3600)
            .then(|| claims.exp - now)
    }
}

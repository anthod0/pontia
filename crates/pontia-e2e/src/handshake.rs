use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit,
    aead::{Aead, Payload},
};
use ed25519_dalek::VerifyingKey;
use hkdf::Hkdf;
use hpke::{
    Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable,
    aead::ChaCha20Poly1305 as HpkeAead, kdf::HkdfSha256, kem::X25519HkdfSha256,
};
use rand_core::SeedableRng;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::{
    Capability, DeviceId, Direction, Error, Id, RecordDecoder, RecordEncoder, RequestContext,
    Result, VERSION, context, random,
};

type Kem = X25519HkdfSha256;
type PrivateKey = <Kem as KemTrait>::PrivateKey;
type PublicKey = <Kem as KemTrait>::PublicKey;
const HANDSHAKE_PREFIX: usize = 1 + 16 + 8 + 32 + 32 + 32 + 32;
pub const HANDSHAKE_BYTES: usize = HANDSHAKE_PREFIX + crate::CAPABILITY_BYTES + 16;
const CONFIRMATION_BYTES: usize = 32 + 32 + 16;

pub struct DeviceIdentity {
    pub(crate) device_id: DeviceId,
    pub(crate) key_version: u64,
    secret: PrivateKey,
}

impl DeviceIdentity {
    pub fn generate(device_id: DeviceId, key_version: u64) -> Result<Self> {
        if key_version == 0 {
            return Err(Error::Protocol);
        }
        let seed = Zeroizing::new(random::<32>()?);
        let (secret, _) = Kem::derive_keypair(seed.as_ref());
        Ok(Self {
            device_id,
            key_version,
            secret,
        })
    }

    /// Native persistence boundary; not exposed by WASM.
    pub fn from_private_bytes(device_id: DeviceId, key_version: u64, bytes: &[u8]) -> Result<Self> {
        if key_version == 0 {
            return Err(Error::Protocol);
        }
        let secret = PrivateKey::from_bytes(bytes).map_err(|_| Error::Protocol)?;
        Ok(Self {
            device_id,
            key_version,
            secret,
        })
    }

    pub fn private_bytes(&self) -> Zeroizing<Vec<u8>> {
        Zeroizing::new(self.secret.to_bytes().to_vec())
    }
    pub fn public_key(&self) -> [u8; 32] {
        Kem::sk_to_pk(&self.secret).to_bytes().into()
    }

    /// Validates all authorization before returning any session state.
    pub(crate) fn accept(
        &self,
        wire: &[u8],
        trusted_key: &VerifyingKey,
        now: u64,
    ) -> Result<HandshakeAccepted> {
        if wire.len() != HANDSHAKE_BYTES || wire[0] != VERSION {
            return Err(Error::Protocol);
        }
        let device_id: DeviceId = wire[1..17].try_into().unwrap();
        let key_version = u64::from_be_bytes(wire[17..25].try_into().unwrap());
        if device_id != self.device_id || key_version != self.key_version {
            return Err(Error::Authentication);
        }
        let handshake_id: Id = wire[25..57].try_into().unwrap();
        let browser_key: [u8; 32] = wire[57..89].try_into().unwrap();
        let authorization_id: Id = wire[89..121].try_into().unwrap();
        let enc = <Kem as KemTrait>::EncappedKey::from_bytes(&wire[121..153])
            .map_err(|_| Error::Protocol)?;
        let public = PublicKey::from_bytes(&browser_key).map_err(|_| Error::Protocol)?;
        let info = handshake_info(
            &device_id,
            key_version,
            &handshake_id,
            &browser_key,
            &authorization_id,
        );
        let mut receiver = hpke::setup_receiver::<HpkeAead, HkdfSha256, Kem>(
            &OpModeR::Auth(public),
            &self.secret,
            &enc,
            &info,
        )
        .map_err(|_| Error::Authentication)?;
        let plaintext = receiver
            .open(&wire[HANDSHAKE_PREFIX..], &info)
            .map_err(|_| Error::Authentication)?;
        let capability = Capability::decode(&plaintext)?;
        capability.verify(trusted_key, now)?;
        if capability.device_id != device_id
            || capability.key_version != key_version
            || capability.browser_public_key != browser_key
            || capability.authorization_id != authorization_id
        {
            return Err(Error::Authentication);
        }
        let mut master = Zeroizing::new([0; 32]);
        let export = context(b"session-master", &[&info, &wire[121..153]]);
        receiver
            .export(&export, master.as_mut())
            .map_err(|_| Error::Authentication)?;
        let session_id = random()?;
        // A fresh session ID is in the confirmation KDF, including on a replayed handshake.
        // Repeated HPKE requests therefore never reuse a confirmation key/nonce pair.
        let binding = context(b"confirmation", &[&info, &session_id]);
        let material = confirmation_material(&master, &binding);
        let ciphertext = ChaCha20Poly1305::new_from_slice(&material[..32])
            .unwrap()
            .encrypt(
                material[32..].into(),
                Payload {
                    msg: &handshake_id,
                    aad: &binding,
                },
            )
            .map_err(|_| Error::Authentication)?;
        let mut confirmation = session_id.to_vec();
        confirmation.extend_from_slice(&ciphertext);
        Ok(HandshakeAccepted {
            session_id,
            browser_public_key: browser_key,
            device_id,
            key_version,
            master,
            confirmation,
        })
    }
}

fn handshake_info(
    device_id: &DeviceId,
    version: u64,
    handshake_id: &Id,
    browser_key: &[u8; 32],
    authorization_id: &Id,
) -> Vec<u8> {
    context(
        b"hpke-auth-session",
        &[
            device_id,
            &version.to_be_bytes(),
            handshake_id,
            browser_key,
            authorization_id,
        ],
    )
}

fn confirmation_material(master: &[u8; 32], binding: &[u8]) -> Zeroizing<[u8; 44]> {
    let mut material = Zeroizing::new([0; 44]);
    Hkdf::<Sha256>::from_prk(master)
        .unwrap()
        .expand(binding, material.as_mut())
        .unwrap();
    material
}

pub(crate) struct HandshakeAccepted {
    pub(crate) session_id: Id,
    pub(crate) browser_public_key: [u8; 32],
    pub(crate) device_id: DeviceId,
    pub(crate) key_version: u64,
    pub(crate) master: Zeroizing<[u8; 32]>,
    pub(crate) confirmation: Vec<u8>,
}

/// A browser identity is single-use. Failure requires a new identity and capability.
pub struct BrowserIdentity {
    secret: PrivateKey,
}

impl BrowserIdentity {
    pub fn generate() -> Result<Self> {
        let seed = Zeroizing::new(random::<32>()?);
        let (secret, _) = Kem::derive_keypair(seed.as_ref());
        Ok(Self { secret })
    }

    pub fn public_key(&self) -> [u8; 32] {
        Kem::sk_to_pk(&self.secret).to_bytes().into()
    }

    pub fn start(
        self,
        device_public: &[u8; 32],
        signed_capability: &[u8],
    ) -> Result<BrowserHandshake> {
        let capability = Capability::decode(signed_capability)?;
        if capability.browser_public_key != self.public_key() {
            return Err(Error::Authentication);
        }
        let handshake_id = random()?;
        let info = handshake_info(
            &capability.device_id,
            capability.key_version,
            &handshake_id,
            &self.public_key(),
            &capability.authorization_id,
        );
        let device_public = PublicKey::from_bytes(device_public).map_err(|_| Error::Protocol)?;
        let seed = Zeroizing::new(random::<32>()?);
        let mut rng = rand_chacha::ChaCha20Rng::from_seed(*seed);
        let (enc, mut sender) = hpke::setup_sender::<HpkeAead, HkdfSha256, Kem, _>(
            &OpModeS::Auth((self.secret.clone(), Kem::sk_to_pk(&self.secret))),
            &device_public,
            &info,
            &mut rng,
        )
        .map_err(|_| Error::Authentication)?;
        let ciphertext = sender
            .seal(signed_capability, &info)
            .map_err(|_| Error::Authentication)?;
        let mut master = Zeroizing::new([0; 32]);
        let export = context(b"session-master", &[&info, enc.to_bytes().as_slice()]);
        sender
            .export(&export, master.as_mut())
            .map_err(|_| Error::Authentication)?;
        let mut wire = vec![VERSION];
        wire.extend_from_slice(&capability.device_id);
        wire.extend_from_slice(&capability.key_version.to_be_bytes());
        wire.extend_from_slice(&handshake_id);
        wire.extend_from_slice(&capability.browser_public_key);
        wire.extend_from_slice(&capability.authorization_id);
        wire.extend_from_slice(&enc.to_bytes());
        wire.extend_from_slice(&ciphertext);
        Ok(BrowserHandshake {
            wire,
            master,
            info,
            handshake_id,
            device_id: capability.device_id,
            key_version: capability.key_version,
        })
    }
}

pub struct BrowserHandshake {
    wire: Vec<u8>,
    master: Zeroizing<[u8; 32]>,
    info: Vec<u8>,
    handshake_id: Id,
    device_id: DeviceId,
    key_version: u64,
}

impl BrowserHandshake {
    pub fn request_bytes(&self) -> &[u8] {
        &self.wire
    }

    pub fn confirm(self, response: &[u8]) -> Result<BrowserSession> {
        if response.len() != CONFIRMATION_BYTES {
            return Err(Error::Protocol);
        }
        let session_id: Id = response[..32].try_into().unwrap();
        let binding = context(b"confirmation", &[&self.info, &session_id]);
        let material = confirmation_material(&self.master, &binding);
        let plaintext = ChaCha20Poly1305::new_from_slice(&material[..32])
            .unwrap()
            .decrypt(
                material[32..].into(),
                Payload {
                    msg: &response[32..],
                    aad: &binding,
                },
            )
            .map_err(|_| Error::Authentication)?;
        if plaintext != self.handshake_id {
            return Err(Error::Authentication);
        }
        Ok(BrowserSession {
            master: self.master,
            device_id: self.device_id,
            key_version: self.key_version,
            session_id,
        })
    }
}

pub struct BrowserSession {
    master: Zeroizing<[u8; 32]>,
    device_id: DeviceId,
    key_version: u64,
    session_id: Id,
}

impl BrowserSession {
    pub fn session_id(&self) -> Id {
        self.session_id
    }
    pub fn request(&self) -> Result<(RequestContext, RecordEncoder, RecordDecoder)> {
        let request = RequestContext {
            device_id: self.device_id,
            key_version: self.key_version,
            session_id: self.session_id,
            request_id: random()?,
        };
        let encoder = RecordEncoder::new(&self.master, &request, Direction::Request);
        let decoder = RecordDecoder::new(&self.master, &request, Direction::Response);
        Ok((request, encoder, decoder))
    }
}

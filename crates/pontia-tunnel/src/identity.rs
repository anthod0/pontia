use std::{fs, io::Write, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{Error, Result, protocol};

pub struct DeviceIdentity {
    device_id: Uuid,
    key: SigningKey,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredIdentity {
    device_id: Uuid,
    secret_key: String,
}

impl DeviceIdentity {
    pub fn generate() -> Result<Self> {
        Ok(Self {
            device_id: Uuid::new_v4(),
            key: SigningKey::from_bytes(&protocol::nonce()?),
        })
    }

    pub fn device_id(&self) -> Uuid {
        self.device_id
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    pub fn authenticate(&self, nonce: &[u8; 32]) -> protocol::Message {
        let signature = self
            .key
            .sign(&protocol::signing_payload(self.device_id, nonce));
        protocol::Message::Authenticate {
            device_id: self.device_id,
            signature: STANDARD.encode(signature.to_bytes()),
        }
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => return Self::decode(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let parent = path
            .parent()
            .ok_or(Error::Protocol("identity path needs a parent"))?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(parent)?;
        let identity = Self::generate()?;
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        serde_json::to_writer(
            &mut file,
            &StoredIdentity {
                device_id: identity.device_id,
                secret_key: STANDARD.encode(identity.key.to_bytes()),
            },
        )?;
        file.flush()?;
        file.as_file().sync_all()?;
        let identity = match file.persist_noclobber(path) {
            Ok(_) => identity,
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                Self::decode(&fs::read(path)?)?
            }
            Err(error) => return Err(error.error.into()),
        };
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok(identity)
    }

    fn decode(bytes: &[u8]) -> Result<Self> {
        let stored: StoredIdentity = serde_json::from_slice(bytes)?;
        let secret: [u8; 32] = STANDARD
            .decode(&stored.secret_key)
            .ok()
            .and_then(|bytes| bytes.try_into().ok())
            .ok_or(Error::Protocol("invalid device key"))?;
        Ok(Self {
            device_id: stored.device_id,
            key: SigningKey::from_bytes(&secret),
        })
    }
}

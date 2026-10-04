use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit,
    aead::{Aead, Payload},
};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::{DeviceId, Error, Id, Result, context};

pub const MAX_RECORD_PLAINTEXT: usize = 16 * 1024;
const TAG_BYTES: usize = 16;
const PREFIX_BYTES: usize = 5;
// A direction can encrypt at most 2^32 records (64 TiB), including the final record.
const MAX_RECORDS: u64 = 1 << 32;

#[derive(Clone, Copy)]
pub enum Direction {
    Request,
    Response,
}

impl Direction {
    fn label(self) -> &'static [u8] {
        match self {
            Self::Request => b"request",
            Self::Response => b"response",
        }
    }
}

#[derive(Clone)]
pub struct RequestContext {
    pub device_id: DeviceId,
    pub key_version: u64,
    pub session_id: Id,
    pub request_id: Id,
}

impl RequestContext {
    fn binding(&self, direction: Direction) -> Vec<u8> {
        context(
            b"business",
            &[
                &self.device_id,
                &self.key_version.to_be_bytes(),
                &self.session_id,
                &self.request_id,
                direction.label(),
            ],
        )
    }
}

struct RecordKey {
    material: Zeroizing<[u8; 44]>,
    binding: Vec<u8>,
    sequence: u64,
    revoked: Option<Arc<AtomicBool>>,
}

impl RecordKey {
    fn new(master: &[u8; 32], request: &RequestContext, direction: Direction) -> Self {
        let binding = request.binding(direction);
        let mut material = Zeroizing::new([0; 44]);
        Hkdf::<Sha256>::from_prk(master)
            .expect("32-byte PRK")
            .expand(&binding, material.as_mut())
            .expect("44-byte HKDF output");
        Self {
            material,
            binding,
            sequence: 0,
            revoked: None,
        }
    }

    fn nonce(&self) -> Result<[u8; 12]> {
        if self
            .revoked
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
        {
            return Err(Error::UnknownSession);
        }
        if self.sequence >= MAX_RECORDS {
            return Err(Error::Capacity);
        }
        let mut nonce: [u8; 12] = self.material[32..].try_into().unwrap();
        for (dst, src) in nonce[4..].iter_mut().zip(self.sequence.to_be_bytes()) {
            *dst ^= src;
        }
        Ok(nonce)
    }

    fn aad(&self, prefix: &[u8]) -> Vec<u8> {
        context(
            b"record",
            &[&self.binding, &self.sequence.to_be_bytes(), prefix],
        )
    }

    fn cipher(&self) -> ChaCha20Poly1305 {
        ChaCha20Poly1305::new_from_slice(&self.material[..32]).expect("32-byte key")
    }
}

/// Not Clone: a direction's nonce sequence must have exactly one owner.
pub struct RecordEncoder {
    key: Option<RecordKey>,
}

impl RecordEncoder {
    pub(crate) fn new(master: &[u8; 32], request: &RequestContext, direction: Direction) -> Self {
        Self {
            key: Some(RecordKey::new(master, request, direction)),
        }
    }

    pub(crate) fn bind_revocation(&mut self, flag: Arc<AtomicBool>) {
        if let Some(key) = self.key.as_mut() {
            key.revoked = Some(flag);
        }
    }

    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        if plaintext.is_empty() || plaintext.len() > MAX_RECORD_PLAINTEXT {
            self.key = None;
            return Err(Error::Protocol);
        }
        self.encode(plaintext, false)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>> {
        self.encode(&[], true)
    }

    fn encode(&mut self, plaintext: &[u8], final_record: bool) -> Result<Vec<u8>> {
        let mut key = self.key.take().ok_or(Error::Closed)?;
        let length = (plaintext.len() + TAG_BYTES) as u32;
        let mut prefix = length.to_be_bytes().to_vec();
        prefix.push(u8::from(final_record));
        let nonce = key.nonce()?;
        let ciphertext = key
            .cipher()
            .encrypt(
                (&nonce).into(),
                Payload {
                    msg: plaintext,
                    aad: &key.aad(&prefix),
                },
            )
            .map_err(|_| Error::Authentication)?;
        let mut output = prefix;
        output.extend_from_slice(&ciphertext);
        key.sequence += 1;
        if !final_record {
            self.key = Some(key);
        }
        Ok(output)
    }
}

/// Feed at most one record at a time. Network read boundaries are irrelevant.
pub struct RecordDecoder {
    key: Option<RecordKey>,
    buffer: Vec<u8>,
    expected: usize,
    complete: bool,
}

impl RecordDecoder {
    pub(crate) fn new(master: &[u8; 32], request: &RequestContext, direction: Direction) -> Self {
        Self {
            key: Some(RecordKey::new(master, request, direction)),
            buffer: Vec::new(),
            expected: PREFIX_BYTES,
            complete: false,
        }
    }

    pub(crate) fn bind_revocation(&mut self, flag: Arc<AtomicBool>) {
        if let Some(key) = self.key.as_mut() {
            key.revoked = Some(flag);
        }
    }

    /// Returns bytes consumed and, when available, one authenticated plaintext record.
    /// The caller must process that record before feeding the unconsumed suffix.
    pub fn feed(&mut self, input: &[u8]) -> Result<(usize, Option<Vec<u8>>)> {
        let result = self.feed_inner(input);
        if result.is_err() {
            self.key = None;
            self.buffer.clear();
            self.complete = false;
        }
        result
    }

    fn feed_inner(&mut self, input: &[u8]) -> Result<(usize, Option<Vec<u8>>)> {
        if self.complete {
            return if input.is_empty() {
                Ok((0, None))
            } else {
                Err(Error::Protocol)
            };
        }
        let key = self.key.as_mut().ok_or(Error::Closed)?;
        let mut consumed = 0;
        loop {
            let take = (self.expected - self.buffer.len()).min(input.len() - consumed);
            self.buffer
                .extend_from_slice(&input[consumed..consumed + take]);
            consumed += take;
            if self.buffer.len() < self.expected {
                return Ok((consumed, None));
            }
            if self.expected == PREFIX_BYTES {
                let len = u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
                let flags = self.buffer[4];
                if !(TAG_BYTES..=MAX_RECORD_PLAINTEXT + TAG_BYTES).contains(&len)
                    || flags > 1
                    || (flags == 1 && len != TAG_BYTES)
                    || (flags == 0 && len == TAG_BYTES)
                {
                    return Err(Error::Protocol);
                }
                self.expected += len;
                continue;
            }
            let nonce = key.nonce()?;
            let plaintext = key
                .cipher()
                .decrypt(
                    (&nonce).into(),
                    Payload {
                        msg: &self.buffer[PREFIX_BYTES..],
                        aad: &key.aad(&self.buffer[..PREFIX_BYTES]),
                    },
                )
                .map_err(|_| Error::Authentication)?;
            key.sequence += 1;
            self.complete = self.buffer[4] == 1;
            self.buffer.clear();
            self.expected = PREFIX_BYTES;
            if self.complete {
                self.key = None;
                return Ok((consumed, None));
            }
            return Ok((consumed, Some(plaintext)));
        }
    }

    /// Must be called on outer body EOF. An authenticated final record is mandatory.
    pub fn finish(&mut self) -> Result<()> {
        self.key = None;
        if self.complete && self.buffer.is_empty() {
            Ok(())
        } else {
            Err(Error::Truncated)
        }
    }
}

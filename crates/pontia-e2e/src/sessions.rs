use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use ed25519_dalek::VerifyingKey;
use zeroize::Zeroizing;

use crate::{
    DeviceIdentity, Direction, Error, Id, RecordDecoder, RecordEncoder, RequestContext, Result,
};

pub const IDLE_SECONDS: u64 = 4 * 60 * 60;
pub const MAX_SESSIONS: usize = 64;
pub const MAX_STREAMS: usize = 64;
pub const MAX_REQUESTS: usize = 65_536;
pub const MAX_HANDSHAKES_PER_SECOND: usize = 16;

type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;
struct Usage {
    active: usize,
    last_activity: u64,
}
struct Session {
    master: Zeroizing<[u8; 32]>,
    _browser_public_key: [u8; 32],
    usage: Arc<Mutex<Usage>>,
    requests: HashSet<Id>,
    revoked: Arc<AtomicBool>,
}

/// Must be held for both directions, including after the handler returns its response head.
/// Dropping it ends stream activity; it does not change application state.
pub struct StreamLease {
    usage: Arc<Mutex<Usage>>,
    clock: Clock,
    revoked: Arc<AtomicBool>,
    streams: Arc<AtomicUsize>,
    accepted: bool,
}
impl StreamLease {
    /// Call after incrementally decoding a valid request head, before dispatch.
    /// Authentication alone reserves replay/stream resources but cannot keep a session alive.
    pub fn accept_head(&mut self, head: &crate::bhttp::Head) -> Result<()> {
        self.check()?;
        if self.accepted {
            return Err(Error::Closed);
        }
        if !matches!(head, crate::bhttp::Head::Request { .. }) {
            return Err(Error::Protocol);
        }
        crate::bhttp::validate_head(head)?;
        let mut usage = self.usage.lock().unwrap();
        self.check()?;
        usage.active += 1;
        usage.last_activity = (self.clock)();
        self.accepted = true;
        Ok(())
    }

    pub fn check(&self) -> Result<()> {
        if self.revoked.load(Ordering::Relaxed) {
            Err(Error::UnknownSession)
        } else {
            Ok(())
        }
    }
}
impl Drop for StreamLease {
    fn drop(&mut self) {
        self.streams.fetch_sub(1, Ordering::Relaxed);
        if self.accepted {
            let mut usage = self.usage.lock().unwrap();
            usage.active -= 1;
            usage.last_activity = (self.clock)();
        }
    }
}

/// Bounded process-local session state. Replay IDs are never evicted from a live session.
/// At the request budget the client must establish a new session, not reuse old keys.
pub struct DeviceSessions {
    identity: DeviceIdentity,
    trusted_key: VerifyingKey,
    sessions: HashMap<Id, Session>,
    clock: Clock,
    handshake_window_start: u64,
    handshake_attempts: usize,
    streams: Arc<AtomicUsize>,
}

impl Drop for DeviceSessions {
    fn drop(&mut self) {
        for session in self.sessions.values() {
            session.revoked.store(true, Ordering::Relaxed);
        }
    }
}

impl DeviceSessions {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(identity: DeviceIdentity, trusted_key: VerifyingKey) -> Self {
        let started = Instant::now();
        Self::with_clock(
            identity,
            trusted_key,
            Arc::new(move || started.elapsed().as_secs()),
        )
    }

    /// Inject a monotonic seconds clock. Authorization uses wall-clock time separately.
    pub fn with_clock(identity: DeviceIdentity, trusted_key: VerifyingKey, clock: Clock) -> Self {
        let handshake_window_start = clock();
        Self {
            identity,
            trusted_key,
            sessions: HashMap::new(),
            clock,
            handshake_window_start,
            handshake_attempts: 0,
            streams: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn handshake(&mut self, wire: &[u8], unix_seconds: u64) -> Result<Vec<u8>> {
        let now = (self.clock)();
        if now.saturating_sub(self.handshake_window_start) >= 1 {
            self.handshake_window_start = now;
            self.handshake_attempts = 0;
        }
        if self.handshake_attempts >= MAX_HANDSHAKES_PER_SECOND {
            return Err(Error::Capacity);
        }
        self.handshake_attempts += 1;
        self.reap();
        if self.sessions.len() >= MAX_SESSIONS {
            return Err(Error::Capacity);
        }
        let accepted = self
            .identity
            .accept(wire, &self.trusted_key, unix_seconds)?;
        if self.sessions.contains_key(&accepted.session_id) {
            return Err(Error::Capacity);
        }
        debug_assert_eq!(accepted.device_id, self.identity.device_id);
        debug_assert_eq!(accepted.key_version, self.identity.key_version);
        self.sessions.insert(
            accepted.session_id,
            Session {
                master: accepted.master,
                _browser_public_key: accepted.browser_public_key,
                usage: Arc::new(Mutex::new(Usage {
                    active: 0,
                    last_activity: (self.clock)(),
                })),
                requests: HashSet::new(),
                revoked: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(accepted.confirmation)
    }

    /// Authenticate one complete first record before reserving replay/stream resources.
    /// Returns its plaintext for incremental bHTTP decoding. Admission does not refresh
    /// idle time: accept the decoded request head through the lease before dispatch.
    pub fn request(
        &mut self,
        session_id: Id,
        request_id: Id,
        first_record: &[u8],
    ) -> Result<(Vec<u8>, RecordDecoder, RecordEncoder, StreamLease)> {
        self.reap();
        if self.streams.load(Ordering::Relaxed) >= MAX_STREAMS {
            return Err(Error::Capacity);
        }
        let session = self
            .sessions
            .get_mut(&session_id)
            .ok_or(Error::UnknownSession)?;
        if session.requests.contains(&request_id) {
            return Err(Error::Replay);
        }
        if session.requests.len() >= MAX_REQUESTS {
            return Err(Error::Capacity);
        }
        let request = RequestContext {
            device_id: self.identity.device_id,
            key_version: self.identity.key_version,
            session_id,
            request_id,
        };
        let mut decoder = RecordDecoder::new(&session.master, &request, Direction::Request);
        let (consumed, plaintext) = decoder.feed(first_record)?;
        if consumed != first_record.len() {
            return Err(Error::Protocol);
        }
        let plaintext = plaintext.ok_or(Error::Truncated)?;
        let mut encoder = RecordEncoder::new(&session.master, &request, Direction::Response);
        decoder.bind_revocation(session.revoked.clone());
        encoder.bind_revocation(session.revoked.clone());
        session.requests.insert(request_id);
        self.streams.fetch_add(1, Ordering::Relaxed);
        let lease = StreamLease {
            usage: session.usage.clone(),
            clock: self.clock.clone(),
            revoked: session.revoked.clone(),
            streams: self.streams.clone(),
            accepted: false,
        };
        Ok((plaintext, decoder, encoder, lease))
    }

    pub fn reap(&mut self) {
        let now = (self.clock)();
        self.sessions.retain(|_, session| {
            let usage = session.usage.lock().unwrap();
            let keep = usage.active > 0 || now.saturating_sub(usage.last_activity) < IDLE_SECONDS;
            if !keep {
                session.revoked.store(true, Ordering::Relaxed);
            }
            keep
        });
    }

    /// Called only after a successful local identity switch, not on Cloud logout.
    pub fn replace_identity(&mut self, identity: DeviceIdentity) {
        for session in self.sessions.values() {
            session.revoked.store(true, Ordering::Relaxed);
        }
        self.sessions.clear();
        self.identity = identity;
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

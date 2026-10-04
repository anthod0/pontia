mod support;
use ed25519_dalek::SigningKey;
use pontia_e2e::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unauthenticated_handshake_attempts_have_a_device_wide_rate_budget() {
    let ticks = Arc::new(AtomicU64::new(0));
    let clock = ticks.clone();
    let identity = DeviceIdentity::generate([1; 16], 1).unwrap();
    let signing = SigningKey::from_bytes(&[2; 32]);
    let mut sessions = DeviceSessions::with_clock(
        identity,
        signing.verifying_key(),
        Arc::new(move || clock.load(Ordering::Relaxed)),
    );
    for _ in 0..MAX_HANDSHAKES_PER_SECOND {
        assert_eq!(sessions.handshake(&[], 100), Err(Error::Protocol));
    }
    assert_eq!(sessions.handshake(&[], 100), Err(Error::Capacity));
    assert_eq!(sessions.session_count(), 0);
    ticks.store(1, Ordering::Relaxed);
    assert_eq!(sessions.handshake(&[], 100), Err(Error::Protocol));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn replay_capacity_never_evicts_old_ids_to_reexecute_a_request() {
    let identity = DeviceIdentity::generate([1; 16], 1).unwrap();
    let public = identity.public_key();
    let signing = SigningKey::from_bytes(&[2; 32]);
    let browser = BrowserIdentity::generate().unwrap();
    let cap = support::capability(&signing, [1; 16], 1, browser.public_key(), 100);
    let handshake = browser.start(&public, &cap).unwrap();
    let mut sessions =
        DeviceSessions::with_clock(identity, signing.verifying_key(), Arc::new(|| 0));
    let confirmation = sessions.handshake(handshake.request_bytes(), 100).unwrap();
    let browser = handshake.confirm(&confirmation).unwrap();
    let mut original = None;
    for _ in 0..MAX_REQUESTS {
        let (context, mut encoder, _) = browser.request().unwrap();
        let first = encoder.seal(b"authenticated head").unwrap();
        let (_, _, _, lease) = sessions
            .request(context.session_id, context.request_id, &first)
            .unwrap();
        drop(lease);
        if original.is_none() {
            original = Some((context, first));
        }
    }
    let (ctx, first) = original.unwrap();
    assert!(matches!(
        sessions.request(ctx.session_id, ctx.request_id, &first),
        Err(Error::Replay)
    ));
    let (ctx, mut encoder, _) = browser.request().unwrap();
    assert!(matches!(
        sessions.request(
            ctx.session_id,
            ctx.request_id,
            &encoder.seal(b"new head").unwrap()
        ),
        Err(Error::Capacity)
    ));
}

mod support;
use ed25519_dalek::SigningKey;
use http::{HeaderMap, HeaderValue, Method};
use pontia_e2e::{
    bhttp::{self, Decoder, Encoder, Event, Head, Kind},
    *,
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

const DEVICE: DeviceId = [3; 16];
const ISSUED: u64 = 1_800_000_000;
fn signer() -> SigningKey {
    SigningKey::from_bytes(&[9; 32])
}
fn identity() -> DeviceIdentity {
    DeviceIdentity::from_private_bytes(DEVICE, 7, &[5; 32]).unwrap()
}
fn capability(browser: [u8; 32]) -> Vec<u8> {
    support::capability(&signer(), DEVICE, 7, browser, ISSUED)
}
fn handshake() -> BrowserHandshake {
    let browser = BrowserIdentity::generate().unwrap();
    let capability = capability(browser.public_key());
    browser
        .start(&identity().public_key(), &capability)
        .unwrap()
}
fn sessions(clock: &Arc<AtomicU64>) -> DeviceSessions {
    let ticks = clock.clone();
    DeviceSessions::with_clock(
        identity(),
        signer().verifying_key(),
        Arc::new(move || ticks.load(Ordering::Relaxed)),
    )
}
fn established(clock: &Arc<AtomicU64>) -> (BrowserSession, DeviceSessions) {
    let mut sessions = sessions(clock);
    let handshake = handshake();
    let confirmation = sessions
        .handshake(handshake.request_bytes(), ISSUED)
        .unwrap();
    (handshake.confirm(&confirmation).unwrap(), sessions)
}
fn head() -> Head {
    Head::Request {
        method: Method::GET,
        uri: "/api/v1/sessions".parse().unwrap(),
        headers: HeaderMap::new(),
    }
}
fn admission(browser: &BrowserSession, sessions: &mut DeviceSessions) -> StreamLease {
    let (context, mut upload, _) = browser.request().unwrap();
    let (_, bytes) = Encoder::new(&head()).unwrap();
    let (_, _, _, mut lease) = sessions
        .request(
            context.session_id,
            context.request_id,
            &upload.seal(&bytes).unwrap(),
        )
        .unwrap();
    lease.accept_head(&head()).unwrap();
    lease
}
fn decode_records(decoder: &mut RecordDecoder, bytes: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    for chunk in bytes.chunks(1) {
        let (used, plaintext) = decoder.feed(chunk)?;
        assert_eq!(used, chunk.len());
        if let Some(plaintext) = plaintext {
            output.extend_from_slice(&plaintext);
        }
    }
    decoder.finish()?;
    Ok(output)
}
fn decode_http(bytes: &[u8], kind: Kind) -> Result<(Head, Vec<u8>)> {
    let mut decoder = Decoder::new(kind);
    let mut head = None;
    let mut content = Vec::new();
    for byte in bytes.chunks(1) {
        let (used, event) = decoder.feed(byte)?;
        assert_eq!(used, 1);
        match event {
            Some(Event::Head(value)) => head = Some(value),
            Some(Event::Content(bytes)) => content.extend_from_slice(&bytes),
            _ => (),
        }
    }
    decoder.finish()?;
    Ok((head.unwrap(), content))
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn signed_capability_accepts_only_the_sixty_second_window() {
    let cap = Capability::decode(&capability([1; 32])).unwrap();
    assert!(cap.verify(&signer().verifying_key(), ISSUED).is_ok());
    assert!(cap.verify(&signer().verifying_key(), ISSUED + 59).is_ok());
    assert_eq!(
        cap.verify(&signer().verifying_key(), ISSUED + 60),
        Err(Error::AuthorizationWindow)
    );
    assert_eq!(
        cap.verify(&signer().verifying_key(), ISSUED - 1),
        Err(Error::AuthorizationWindow)
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn capability_signed_by_an_untrusted_key_is_rejected() {
    let cap = Capability::decode(&capability([1; 32])).unwrap();
    assert_eq!(
        cap.verify(&SigningKey::from_bytes(&[8; 32]).verifying_key(), ISSUED),
        Err(Error::Authentication)
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn copied_capability_does_not_authorize_another_private_key() {
    let authorized = BrowserIdentity::generate().unwrap();
    let attacker = BrowserIdentity::generate().unwrap();
    assert!(matches!(
        attacker.start(
            &identity().public_key(),
            &capability(authorized.public_key())
        ),
        Err(Error::Authentication)
    ));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn tampering_with_each_handshake_binding_creates_no_session() {
    for index in [
        0,
        1,
        17,
        25,
        57,
        89,
        121,
        153,
        153 + CAPABILITY_BYTES + 16 - 1,
    ] {
        let handshake = handshake();
        let mut wire = handshake.request_bytes().to_vec();
        wire[index] ^= 1;
        let mut sessions = sessions(&Arc::new(AtomicU64::new(0)));
        assert!(sessions.handshake(&wire, ISSUED).is_err(), "index {index}");
        assert_eq!(sessions.session_count(), 0);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn expired_authorization_creates_no_session() {
    let handshake = handshake();
    let mut sessions = sessions(&Arc::new(AtomicU64::new(0)));
    assert_eq!(
        sessions.handshake(handshake.request_bytes(), ISSUED + 60),
        Err(Error::AuthorizationWindow)
    );
    assert_eq!(sessions.session_count(), 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn confirmation_tampering_is_rejected() {
    let mut sessions = sessions(&Arc::new(AtomicU64::new(0)));
    let pending = handshake();
    let mut confirmation = sessions.handshake(pending.request_bytes(), ISSUED).unwrap();
    confirmation[40] ^= 1;
    assert!(matches!(
        pending.confirm(&confirmation),
        Err(Error::Authentication)
    ));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn replayed_handshakes_have_independent_confirmation_material() {
    let mut sessions = sessions(&Arc::new(AtomicU64::new(0)));
    let pending = handshake();
    let first = sessions.handshake(pending.request_bytes(), ISSUED).unwrap();
    let second = sessions.handshake(pending.request_bytes(), ISSUED).unwrap();
    assert_ne!(&first[..32], &second[..32]);
    assert_ne!(&first[32..], &second[32..]);
    assert_eq!(sessions.session_count(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn lost_confirmation_does_not_prevent_a_fresh_handshake() {
    let mut sessions = sessions(&Arc::new(AtomicU64::new(0)));
    let lost = handshake();
    sessions.handshake(lost.request_bytes(), ISSUED).unwrap();
    drop(lost);
    let fresh = handshake();
    let confirmation = sessions.handshake(fresh.request_bytes(), ISSUED).unwrap();
    let browser = fresh.confirm(&confirmation).unwrap();
    let lease = admission(&browser, &mut sessions);
    assert!(lease.check().is_ok());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn ordinary_http_roundtrips_under_arbitrary_record_network_splitting() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let mut headers = HeaderMap::new();
    headers.insert("idempotency-key", HeaderValue::from_static("command-1"));
    let head = Head::Request {
        method: Method::POST,
        uri: "/api/v1/sessions?name=private".parse().unwrap(),
        headers,
    };
    let (mut http, bytes) = Encoder::new(&head).unwrap();
    let (context, mut encoder, _) = browser.request().unwrap();
    let first = encoder.seal(&bytes).unwrap();
    let (mut plaintext, mut decoder, _, _lease) = device
        .request(context.session_id, context.request_id, &first)
        .unwrap();
    let payload = b"{\"private\":\"business input\"}";
    let mut upload = encoder.seal(&http.content(payload).unwrap()).unwrap();
    upload.extend_from_slice(&encoder.seal(&http.finish().unwrap()).unwrap());
    upload.extend_from_slice(&encoder.finish().unwrap());
    plaintext.extend_from_slice(&decode_records(&mut decoder, &upload).unwrap());
    let (head, actual) = decode_http(&plaintext, Kind::Request).unwrap();
    assert_eq!(actual, payload);
    match head {
        Head::Request {
            method,
            uri,
            headers,
        } => {
            assert_eq!(method, Method::POST);
            assert_eq!(uri.to_string(), "/api/v1/sessions?name=private");
            assert_eq!(headers["idempotency-key"], "command-1");
        }
        _ => panic!("expected request"),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn streaming_response_preserves_business_status_and_opaque_sse_bytes() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (context, mut encoder, mut decoder) = browser.request().unwrap();
    let (_, _, mut response, _lease) = device
        .request(
            context.session_id,
            context.request_id,
            &encoder.seal(b"head").unwrap(),
        )
        .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("text/event-stream"),
    );
    let (mut http, bytes) = Encoder::new(&Head::Response {
        status: 409,
        headers,
    })
    .unwrap();
    let mut wire = response.seal(&bytes).unwrap();
    let events = [
        b"event: changed\ndata: secret\n\n".as_slice(),
        b"event: done\ndata: private\n\n".as_slice(),
    ];
    for event in events {
        wire.extend_from_slice(&response.seal(&http.content(event).unwrap()).unwrap());
    }
    wire.extend_from_slice(&response.seal(&http.finish().unwrap()).unwrap());
    wire.extend_from_slice(&response.finish().unwrap());
    let plaintext = decode_records(&mut decoder, &wire).unwrap();
    let (head, body) = decode_http(&plaintext, Kind::Response).unwrap();
    assert_eq!(body, events.concat());
    assert!(matches!(head, Head::Response { status: 409, .. }));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn device_can_respond_before_request_upload_finishes() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (context, mut request, mut response) = browser.request().unwrap();
    let (_, _upload, mut download, _lease) = device
        .request(
            context.session_id,
            context.request_id,
            &request.seal(b"head").unwrap(),
        )
        .unwrap();
    let mut wire = download.seal(b"early rejection").unwrap();
    wire.extend_from_slice(&download.finish().unwrap());
    assert_eq!(
        decode_records(&mut response, &wire).unwrap(),
        b"early rejection"
    );
    assert!(request.seal(b"more upload").is_ok());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn concurrent_requests_cannot_exchange_ciphertext() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (first, mut upload1, _) = browser.request().unwrap();
    let (second, mut upload2, _) = browser.request().unwrap();
    let wire1 = upload1.seal(b"first").unwrap();
    let wire2 = upload2.seal(b"second").unwrap();
    assert!(matches!(
        device.request(second.session_id, second.request_id, &wire1),
        Err(Error::Authentication)
    ));
    assert!(
        device
            .request(first.session_id, first.request_id, &wire1)
            .is_ok()
    );
    assert!(
        device
            .request(second.session_id, second.request_id, &wire2)
            .is_ok()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn replay_is_rejected_before_http_decode() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (ctx, mut upload, _) = browser.request().unwrap();
    let wire = upload.seal(b"head").unwrap();
    let (_, _, _, lease) = device
        .request(ctx.session_id, ctx.request_id, &wire)
        .unwrap();
    drop(lease);
    assert!(matches!(
        device.request(ctx.session_id, ctx.request_id, &wire),
        Err(Error::Replay)
    ));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_ciphertext_does_not_keep_an_idle_session_alive() {
    let clock = Arc::new(AtomicU64::new(0));
    let (browser, mut device) = established(&clock);
    clock.store(IDLE_SECONDS - 1, Ordering::Relaxed);
    let (ctx, mut upload, _) = browser.request().unwrap();
    let mut wire = upload.seal(b"head").unwrap();
    wire[7] ^= 1;
    assert!(matches!(
        device.request(ctx.session_id, ctx.request_id, &wire),
        Err(Error::Authentication)
    ));
    clock.store(IDLE_SECONDS, Ordering::Relaxed);
    device.reap();
    assert_eq!(device.session_count(), 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_authenticated_http_does_not_refresh_idle_time_or_protect_pending_streams() {
    let clock = Arc::new(AtomicU64::new(0));
    let (browser, mut device) = established(&clock);
    clock.store(IDLE_SECONDS - 1, Ordering::Relaxed);
    let (ctx, mut upload, _) = browser.request().unwrap();
    let (plain, _, mut response, mut lease) = device
        .request(
            ctx.session_id,
            ctx.request_id,
            &upload.seal(b"not bHTTP").unwrap(),
        )
        .unwrap();
    let mut decoder = Decoder::new(Kind::Request);
    assert!(decoder.feed(&plain).is_err());
    let invalid = Head::Request {
        method: Method::CONNECT,
        uri: "/api/v1/sessions".parse().unwrap(),
        headers: HeaderMap::new(),
    };
    assert_eq!(lease.accept_head(&invalid), Err(Error::Protocol));
    clock.store(IDLE_SECONDS, Ordering::Relaxed);
    device.reap();
    assert_eq!(device.session_count(), 0);
    assert_eq!(response.seal(b"late"), Err(Error::UnknownSession));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn continuously_authenticated_activity_has_no_absolute_expiry() {
    let clock = Arc::new(AtomicU64::new(0));
    let (browser, mut device) = established(&clock);
    for hour in 1..=8 {
        clock.store(hour * 3600, Ordering::Relaxed);
        drop(admission(&browser, &mut device));
    }
    assert_eq!(device.session_count(), 1);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn active_streams_get_a_full_idle_interval_after_cancellation() {
    let clock = Arc::new(AtomicU64::new(0));
    let (browser, mut device) = established(&clock);
    let lease = admission(&browser, &mut device);
    clock.store(24 * 3600, Ordering::Relaxed);
    device.reap();
    assert_eq!(device.session_count(), 1);
    drop(lease);
    clock.store(24 * 3600 + IDLE_SECONDS - 1, Ordering::Relaxed);
    device.reap();
    assert_eq!(device.session_count(), 1);
    clock.store(24 * 3600 + IDLE_SECONDS, Ordering::Relaxed);
    device.reap();
    assert_eq!(device.session_count(), 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn local_identity_switch_invalidates_existing_direction_keys() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (ctx, mut upload, _) = browser.request().unwrap();
    let (_, mut decoder, mut response, lease) = device
        .request(
            ctx.session_id,
            ctx.request_id,
            &upload.seal(b"head").unwrap(),
        )
        .unwrap();
    let pending = upload.seal(b"pending").unwrap();
    device.replace_identity(DeviceIdentity::from_private_bytes(DEVICE, 8, &[6; 32]).unwrap());
    assert_eq!(lease.check(), Err(Error::UnknownSession));
    assert_eq!(response.seal(b"late"), Err(Error::UnknownSession));
    assert!(matches!(decoder.feed(&pending), Err(Error::UnknownSession)));
    assert_eq!(device.session_count(), 0);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn restarted_device_cannot_accept_a_previous_session() {
    let (browser, device) = established(&Arc::new(AtomicU64::new(0)));
    drop(device);
    let mut restarted = sessions(&Arc::new(AtomicU64::new(0)));
    let (ctx, mut upload, _) = browser.request().unwrap();
    assert!(matches!(
        restarted.request(
            ctx.session_id,
            ctx.request_id,
            &upload.seal(b"head").unwrap()
        ),
        Err(Error::UnknownSession)
    ));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn session_capacity_bounds_replayed_handshakes() {
    let clock = Arc::new(AtomicU64::new(0));
    let mut sessions = sessions(&clock);
    let handshake = handshake();
    for index in 0..MAX_SESSIONS {
        clock.store(
            (index / MAX_HANDSHAKES_PER_SECOND) as u64,
            Ordering::Relaxed,
        );
        sessions
            .handshake(handshake.request_bytes(), ISSUED)
            .unwrap();
    }
    clock.store(5, Ordering::Relaxed);
    assert_eq!(
        sessions.handshake(handshake.request_bytes(), ISSUED),
        Err(Error::Capacity)
    );
    assert_eq!(sessions.session_count(), MAX_SESSIONS);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn pending_stream_capacity_remains_bounded_across_idle_reaping() {
    let clock = Arc::new(AtomicU64::new(0));
    let (browser, mut device) = established(&clock);
    let mut leases = Vec::new();
    for _ in 0..MAX_STREAMS {
        let (ctx, mut upload, _) = browser.request().unwrap();
        let (_, _, _, lease) = device
            .request(
                ctx.session_id,
                ctx.request_id,
                &upload.seal(b"head").unwrap(),
            )
            .unwrap();
        leases.push(lease);
    }
    clock.store(IDLE_SECONDS, Ordering::Relaxed);
    device.reap();
    let fresh = handshake();
    let confirmation = device.handshake(fresh.request_bytes(), ISSUED).unwrap();
    let browser = fresh.confirm(&confirmation).unwrap();
    let (ctx, mut upload, _) = browser.request().unwrap();
    let first = upload.seal(b"head").unwrap();
    assert!(matches!(
        device.request(ctx.session_id, ctx.request_id, &first),
        Err(Error::Capacity)
    ));
    leases.pop();
    assert!(
        device
            .request(ctx.session_id, ctx.request_id, &first)
            .is_ok()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn record_authentication_failure_cannot_be_resumed() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (ctx, mut encoder, _) = browser.request().unwrap();
    let (_, mut decoder, _, _lease) = device
        .request(
            ctx.session_id,
            ctx.request_id,
            &encoder.seal(b"head").unwrap(),
        )
        .unwrap();
    let original = encoder.seal(b"next").unwrap();
    let mut changed = original.clone();
    changed[5] ^= 1;
    assert!(matches!(decoder.feed(&changed), Err(Error::Authentication)));
    assert!(matches!(decoder.feed(&original), Err(Error::Closed)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reordered_records_are_rejected() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (ctx, mut encoder, _) = browser.request().unwrap();
    let (_, mut decoder, _, _lease) = device
        .request(
            ctx.session_id,
            ctx.request_id,
            &encoder.seal(b"head").unwrap(),
        )
        .unwrap();
    encoder.seal(b"second").unwrap();
    let third = encoder.seal(b"third").unwrap();
    assert!(matches!(decoder.feed(&third), Err(Error::Authentication)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn duplicate_records_are_rejected() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (ctx, mut encoder, _) = browser.request().unwrap();
    let (_, mut decoder, _, _lease) = device
        .request(
            ctx.session_id,
            ctx.request_id,
            &encoder.seal(b"head").unwrap(),
        )
        .unwrap();
    let second = encoder.seal(b"second").unwrap();
    assert!(decoder.feed(&second).is_ok());
    assert!(matches!(decoder.feed(&second), Err(Error::Authentication)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn incomplete_authenticated_record_is_truncation() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (ctx, mut encoder, _) = browser.request().unwrap();
    let (_, mut decoder, _, _lease) = device
        .request(
            ctx.session_id,
            ctx.request_id,
            &encoder.seal(b"head").unwrap(),
        )
        .unwrap();
    let next = encoder.seal(b"next").unwrap();
    assert!(decoder.feed(&next[..next.len() - 1]).is_ok());
    assert_eq!(decoder.finish(), Err(Error::Truncated));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn record_length_is_bounded_before_allocation() {
    let (browser, mut device) = established(&Arc::new(AtomicU64::new(0)));
    let (ctx, mut encoder, _) = browser.request().unwrap();
    let (_, mut decoder, _, _lease) = device
        .request(
            ctx.session_id,
            ctx.request_id,
            &encoder.seal(b"head").unwrap(),
        )
        .unwrap();
    assert!(matches!(
        decoder.feed(&[255, 255, 255, 255, 0]),
        Err(Error::Protocol)
    ));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn only_canonical_api_paths_are_admitted() {
    for path in [
        "/dashboard",
        "/api/v1/",
        "/api/v1/../sessions",
        "/api/v1/a%2Fb",
        "/api/v1/%73essions",
        "https://evil.test/api/v1/sessions",
    ] {
        assert!(!bhttp::canonical_api_uri(&path.parse().unwrap()), "{path}");
    }
    assert!(bhttp::canonical_api_uri(
        &"/api/v1/sessions?q=a%2Fb".parse().unwrap()
    ));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn connect_is_not_an_allowed_business_method() {
    assert!(
        Encoder::new(&Head::Request {
            method: Method::CONNECT,
            uri: "/api/v1/sessions".parse().unwrap(),
            headers: HeaderMap::new()
        })
        .is_err()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bearer_headers_cannot_assert_a_remote_principal() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer old-cookie"),
    );
    assert!(
        Encoder::new(&Head::Request {
            method: Method::GET,
            uri: "/api/v1/sessions".parse().unwrap(),
            headers
        })
        .is_err()
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn informational_and_invalid_business_statuses_are_rejected() {
    for status in [101, 103, 600] {
        assert!(
            Encoder::new(&Head::Response {
                status,
                headers: HeaderMap::new()
            })
            .is_err()
        );
    }
    assert!(decode_http(&[3, 0x40, 103], Kind::Response).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn known_length_bhttp_downgrade_is_rejected() {
    for bytes in [vec![0], vec![1]] {
        assert!(decode_http(&bytes, Kind::Response).is_err());
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bhttp_trailers_are_rejected() {
    assert!(decode_http(&[3, 0x40, 200, 0, 0, 1, b'x'], Kind::Response).is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bhttp_declared_head_length_is_bounded_before_allocation() {
    let mut decoder = Decoder::new(Kind::Request);
    assert!(matches!(
        decoder.feed(&[2, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
        Err(Error::Capacity)
    ));
    assert!(matches!(decoder.feed(&[2]), Err(Error::Closed)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bhttp_preserves_binary_content_across_multiple_chunks() {
    let (mut encoder, mut bytes) = Encoder::new(&Head::Response {
        status: 200,
        headers: HeaderMap::new(),
    })
    .unwrap();
    let payload = vec![0xff; MAX_RECORD_PLAINTEXT];
    for _ in 0..4 {
        bytes.extend_from_slice(&encoder.content(&payload).unwrap());
    }
    bytes.extend_from_slice(&encoder.finish().unwrap());
    let (_, content) = decode_http(&bytes, Kind::Response).unwrap();
    assert_eq!(content, payload.repeat(4));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bhttp_requires_explicit_content_and_trailer_termination() {
    let (mut encoder, mut bytes) = Encoder::new(&Head::Response {
        status: 200,
        headers: HeaderMap::new(),
    })
    .unwrap();
    bytes.extend_from_slice(&encoder.finish().unwrap());
    for removed in 1..=2 {
        assert!(matches!(
            decode_http(&bytes[..bytes.len() - removed], Kind::Response),
            Err(Error::Truncated)
        ));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn bhttp_rejects_post_completion_padding() {
    let (mut encoder, mut bytes) = Encoder::new(&Head::Response {
        status: 200,
        headers: HeaderMap::new(),
    })
    .unwrap();
    bytes.extend_from_slice(&encoder.finish().unwrap());
    bytes.push(0);
    assert!(matches!(
        decode_http(&bytes, Kind::Response),
        Err(Error::Protocol)
    ));
}

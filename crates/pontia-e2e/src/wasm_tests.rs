#[path = "../tests/support/mod.rs"]
mod support;
use crate::{
    DeviceIdentity, DeviceSessions, Id,
    bhttp::{Decoder, Encoder, Event, Head, Kind},
    wasm::Identity,
};
use ed25519_dalek::SigningKey;
use http::HeaderMap;
use js_sys::{Array, Uint8Array};
use std::sync::Arc;

fn connected() -> (crate::wasm::Session, DeviceSessions) {
    let device = DeviceIdentity::from_private_bytes([1; 16], 1, &[2; 32]).unwrap();
    let signing = SigningKey::from_bytes(&[3; 32]);
    let mut browser = Identity::new().unwrap();
    let cap = support::capability(
        &signing,
        [1; 16],
        1,
        browser.public_key().unwrap().try_into().unwrap(),
        100,
    );
    let mut handshake = browser.start(&device.public_key(), &cap).unwrap();
    let mut sessions = DeviceSessions::with_clock(device, signing.verifying_key(), Arc::new(|| 0));
    let confirmation = sessions
        .handshake(&handshake.bytes().unwrap(), 100)
        .unwrap();
    (handshake.confirm(&confirmation).unwrap(), sessions)
}
fn request(
    session: &crate::wasm::Session,
    device: &mut DeviceSessions,
) -> (
    crate::wasm::Request,
    Vec<u8>,
    crate::RecordDecoder,
    crate::RecordEncoder,
    crate::StreamLease,
) {
    let mut request = session
        .request("POST", "/api/v1/sessions", &Array::new())
        .unwrap();
    let first = request.first_bytes().unwrap();
    let session_id: Id = first[1..33].try_into().unwrap();
    let request_id: Id = first[33..65].try_into().unwrap();
    let (plain, upload, response, lease) = device
        .request(session_id, request_id, &first[65..])
        .unwrap();
    (request, plain, upload, response, lease)
}
fn response_bytes(encoder: &mut crate::RecordEncoder, body: &[u8]) -> Vec<u8> {
    let (mut bhttp, head) = Encoder::new(&Head::Response {
        status: 409,
        headers: HeaderMap::new(),
    })
    .unwrap();
    let mut wire = encoder.seal(&head).unwrap();
    if !body.is_empty() {
        wire.extend_from_slice(&encoder.seal(&bhttp.content(body).unwrap()).unwrap());
    }
    wire.extend_from_slice(&encoder.seal(&bhttp.finish().unwrap()).unwrap());
    wire.extend_from_slice(&encoder.finish().unwrap());
    wire
}
fn feed_response(request: &mut crate::wasm::Request, wire: &[u8]) -> (Option<f64>, Vec<u8>, bool) {
    let mut status = None;
    let mut body = Vec::new();
    let mut ended = false;
    for byte in wire.chunks(1) {
        assert_eq!(request.receive(byte).unwrap(), 1);
        while let Some(event) = request.next_event().unwrap() {
            match event.get(0).as_f64().unwrap() as u8 {
                0 => status = event.get(1).as_f64(),
                1 => body.extend_from_slice(&Uint8Array::new(&event.get(1)).to_vec()),
                2 => ended = true,
                _ => panic!("invalid event"),
            }
        }
    }
    request.finish_response().unwrap();
    (status, body, ended)
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn wasm_upload_preserves_binary_content_and_complete_http_framing() {
    let (session, mut device) = connected();
    let (mut request, mut plaintext, mut upload, _, _lease) = request(&session, &mut device);
    let binary = vec![0xfe; 16 * 1024];
    let mut wire = request.content(&binary).unwrap();
    wire.extend_from_slice(&request.finish_upload().unwrap());
    let mut offset = 0;
    while offset < wire.len() {
        let (used, decoded) = upload.feed(&wire[offset..]).unwrap();
        offset += used;
        if let Some(bytes) = decoded {
            plaintext.extend_from_slice(&bytes);
        }
    }
    upload.finish().unwrap();
    let mut decoder = Decoder::new(Kind::Request);
    let mut actual = Vec::new();
    let mut remaining = plaintext.as_slice();
    while !remaining.is_empty() {
        let (used, event) = decoder.feed(remaining).unwrap();
        remaining = &remaining[used..];
        if let Some(Event::Content(bytes)) = event {
            actual.extend_from_slice(&bytes);
        }
    }
    decoder.finish().unwrap();
    assert_eq!(actual, binary);
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn wasm_response_delivers_business_status_and_opaque_stream_content() {
    let (session, mut device) = connected();
    let (mut request, _, _, mut response, _lease) = request(&session, &mut device);
    request.finish_upload().unwrap();
    let wire = response_bytes(&mut response, b"opaque SSE bytes");
    let (status, body, ended) = feed_response(&mut request, &wire);
    assert_eq!(status, Some(409.0));
    assert_eq!(body, b"opaque SSE bytes");
    assert!(ended);
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn wasm_completed_early_response_preserves_the_upload_direction() {
    let (session, mut device) = connected();
    let (mut request, mut plain, mut upload, mut response, _lease) = request(&session, &mut device);
    let wire = response_bytes(&mut response, b"early response");
    feed_response(&mut request, &wire);
    let mut encrypted = request.content(b"upload after early response").unwrap();
    encrypted.extend_from_slice(&request.finish_upload().unwrap());
    let mut remaining = encrypted.as_slice();
    while !remaining.is_empty() {
        let (used, decoded) = upload.feed(remaining).unwrap();
        remaining = &remaining[used..];
        if let Some(bytes) = decoded {
            plain.extend_from_slice(&bytes);
        }
    }
    upload.finish().unwrap();
    let mut decoder = Decoder::new(Kind::Request);
    let mut remaining = plain.as_slice();
    let mut actual = Vec::new();
    while !remaining.is_empty() {
        let (used, event) = decoder.feed(remaining).unwrap();
        remaining = &remaining[used..];
        if let Some(Event::Content(bytes)) = event {
            actual.extend_from_slice(&bytes);
        }
    }
    decoder.finish().unwrap();
    assert_eq!(actual, b"upload after early response");
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn wasm_stream_authentication_failure_destroys_both_request_direction_handles() {
    let (session, mut device) = connected();
    let (mut request, _, _, mut response, _lease) = request(&session, &mut device);
    let mut wire = response.seal(b"response").unwrap();
    wire[5] ^= 1;
    assert!(request.receive(&wire).is_err());
    assert!(request.content(b"cannot continue").is_err());
    assert!(request.finish_response().is_err());
    assert!(request.next_event().is_err());
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn wasm_identity_cannot_start_a_second_handshake() {
    let device = DeviceIdentity::from_private_bytes([1; 16], 1, &[2; 32]).unwrap();
    let signing = SigningKey::from_bytes(&[3; 32]);
    let mut browser = Identity::new().unwrap();
    let cap = support::capability(
        &signing,
        [1; 16],
        1,
        browser.public_key().unwrap().try_into().unwrap(),
        100,
    );
    browser.start(&device.public_key(), &cap).unwrap();
    assert!(browser.start(&device.public_key(), &cap).is_err());
}

#[wasm_bindgen_test::wasm_bindgen_test]
fn wasm_request_prefix_cannot_be_emitted_twice() {
    let (session, mut device) = connected();
    let (mut request, _, _, _, _lease) = request(&session, &mut device);
    assert!(request.first_bytes().is_err());
}

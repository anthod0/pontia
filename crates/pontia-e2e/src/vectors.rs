use crate::{Direction, RecordDecoder, RecordEncoder, RequestContext};

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn native_and_wasm_match_the_independent_node_crypto_record_vector() {
    // Independently generated with Node crypto HMAC-SHA256 and ChaCha20-Poly1305.
    let master = [0x11; 32];
    let context = RequestContext {
        device_id: [0x22; 16],
        key_version: 7,
        session_id: [0x33; 32],
        request_id: [0x44; 32],
    };
    let expected =
        hex::decode("0000001c00e7b52eb124d97ae48c5f6eb228a1fb3776ded85c8e05b8cdaae6be1d").unwrap();
    let mut encoder = RecordEncoder::new(&master, &context, Direction::Request);
    assert_eq!(encoder.seal(b"opaque\0bytes").unwrap(), expected);
    let mut decoder = RecordDecoder::new(&master, &context, Direction::Request);
    let (consumed, plaintext) = decoder.feed(&expected).unwrap();
    assert_eq!(consumed, expected.len());
    assert_eq!(plaintext.unwrap(), b"opaque\0bytes");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn record_authentication_binds_device_version_session_request_and_direction() {
    let master = [0x11; 32];
    let request = RequestContext {
        device_id: [0x22; 16],
        key_version: 7,
        session_id: [0x33; 32],
        request_id: [0x44; 32],
    };
    let mut encoder = RecordEncoder::new(&master, &request, Direction::Request);
    let wire = encoder.seal(b"opaque\0bytes").unwrap();
    for dimension in 0..5 {
        let mut changed = request.clone();
        let mut direction = Direction::Request;
        match dimension {
            0 => changed.device_id[0] ^= 1,
            1 => changed.key_version += 1,
            2 => changed.session_id[0] ^= 1,
            3 => changed.request_id[0] ^= 1,
            4 => direction = Direction::Response,
            _ => unreachable!(),
        }
        let mut decoder = RecordDecoder::new(&master, &changed, direction);
        assert!(matches!(
            decoder.feed(&wire),
            Err(crate::Error::Authentication)
        ));
    }
}

# E2E core protocol v1

`crates/pontia-e2e` implements the shared native/WASM protocol core used by Public Dashboard and `pontiad`. Cloud signs short-lived handshake capabilities, edge forwards only the two fixed ciphertext transports, and authenticated device requests enter the same External API router as local requests.

The core owns HPKE Auth, capability verification, confirmation, request-key derivation, bounded record framing, a restricted streaming bHTTP profile, and process-local device sessions. Network adapters own HTTP routing, trust-key distribution, persistence, backpressure, cancellation, and automatic reauthorization. The exported WASM handles contain protocol state; they expose no private keys, master secrets, derived keys, or nonce setters.

## Binary contracts

All fixed integers below are big-endian. Device IDs are 16 UUID bytes; authorization, handshake, session, and request IDs are 32 bytes. Public X25519 keys are 32 bytes. Protocol version is the byte `1`.

A capability is 169 bytes:

```text
version:u8 | device_id:16 | key_version:u64 | browser_public_key:32 |
authorization_id:32 | issued_at:u64 | redeem_before:u64 | signature:64
```

The Ed25519 signature covers `pontia-e2e-capability-v1\0` followed by the first 105 bytes. The device checks the signature against its configured trust key, all handshake bindings, `key_version > 0`, and `redeem_before == issued_at + 60`. Unix time must satisfy `issued_at <= now < redeem_before`. There is no capability consumption state.

A session establishment request is 338 bytes:

```text
version:u8 | device_id:16 | key_version:u64 | handshake_id:32 |
browser_public_key:32 | authorization_id:32 | HPKE_enc:32 |
HPKE_encrypted_capability:185
```

HPKE uses Auth mode with DHKEM(X25519, HKDF-SHA256), HKDF-SHA256, and ChaCha20-Poly1305. The browser identity and handshake handles are single-use. Any failure requires a new identity and capability; confirmation is not resumable.

The 80-byte confirmation is `session_id:32 | encrypted_handshake_id:48`. Its key and nonce derive from the exporter master, handshake context, and newly random session ID. Replaying a handshake can create another session, but each confirmation gets independent key/nonce material. The browser cannot request business keys before confirming the device.

The WASM business request prefix is `version:u8 | session_id:32 | request_id:32`, followed by encrypted records. The device ID and key version remain authenticated in key derivation and record context, not in business URLs.

## Contexts and keys

`C(label, fields...)` is `pontia-e2e-v1\0`, followed by the label and each field, **each preceded by its u32 byte length**. Labels below are ASCII bytes; IDs are binary and version is u64.

```text
info = C("hpke-auth-session", device_id, key_version, handshake_id,
         browser_public_key, authorization_id)
master = HPKE-Export(C("session-master", info, HPKE_enc), 32)
confirmation_context = C("confirmation", info, session_id)
confirmation_material = HKDF-Expand(master, confirmation_context, 44)
business_context = C("business", device_id, key_version,
                     session_id, request_id, direction)
record_material = HKDF-Expand(master, business_context, 44)
```

Direction is `request` or `response`. In each 44-byte output, the first 32 bytes are the ChaCha20-Poly1305 key and the remaining 12 are the nonce base. The master is used as a 32-byte HKDF PRK, never as an AEAD key. Independent request handles own independent direction counters and are not cloneable.

A record is `ciphertext_length:u32 | final:u8 | ciphertext`. Associated data is `C("record", business_context, sequence:u64, five_byte_prefix)`. The nonce is the nonce base XOR the sequence encoded in the last eight bytes of a 12-byte value. Sequence starts at zero and each direction permits at most 2^32 records, including its final record.

Nonfinal plaintext is 1–65,536 bytes. The final record authenticates empty plaintext and must be present on EOF. Only flags 0 and 1 are accepted. Lengths are bounded before allocation and authenticated with the ciphertext. Duplicate, reordered, substituted, or corrupted records fail authentication; missing completion is truncation. A failed decoder cannot resume.

[`src/vectors.rs`](../crates/pontia-e2e/src/vectors.rs) contains an independently generated Node crypto record vector executed on both native and WASM targets.

## bHTTP profile and streaming

Request framing indicator is 2; response indicator is 3 (RFC 9292 indeterminate-length). Scheme is `https`, authority is `pontia-device`. Methods and canonical `/api/v1/` paths retain the existing tunnel restrictions. Request headers are limited to `accept`, `content-type`, `idempotency-key`, and `last-event-id`. Response headers are limited to `content-type`, `cache-control`, `etag`, `last-modified`, `content-disposition`, `retry-after`, and `allow`.

Heads are at most 16 KiB. Before invoking the `bhttp` crate's head parser, a framing guard bounds vector lengths and checks that the complete head is available. The crate's async parser is not used because its field-vector allocations have no configurable head budget. Content decoding retains no full body; incoming content chunks can declare at most 1 MiB and outgoing chunks carry at most 64 KiB. Content chunks, encrypted records, and network reads do not have to align.

Only final statuses 200–599, empty trailers, and explicit content/trailer terminators are accepted. Padding, informational responses, upgrades, CONNECT, and known-length messages are rejected. JSON, SSE, and binary files are opaque body bytes. The production adapters accept only bounded request bodies: `pontiad` authenticates and completes the request before invoking a business handler. Responses remain incrementally encrypted and streamed.

Adapters must honor the consumed-byte count, process each returned plaintext/event before feeding its suffix, and call both record and bHTTP completion checks at EOF. WASM `receive` accepts at most one encrypted record and `next_event` drains its decoded events before another record is accepted. Its event arrays are local FFI values, not wire envelopes. Public Dashboard aggregates only existing JSON-style request bodies and does not expose streaming upload.

## Device session limits

A table accepts at most 64 sessions, 64 outstanding streams (including pending head validation), and 16 handshake attempts per monotonic second, including invalid attempts. Pending leases retain their stream budget even if their session is reclaimed. Transport adapters must additionally bound pending ingress, body size, request time, and edge traffic.

Each session retains at most 65,536 request IDs. IDs are never evicted from a live session: reaching capacity rejects new requests and requires another handshake, rather than permitting old ciphertext to execute again. Duplicate IDs are rejected before bHTTP decoding. Invalid first-record authentication neither inserts an ID nor refreshes idle time.

There is no absolute session expiry. Sessions are reclaimed after four idle hours without an accepted business stream. First-record authentication reserves replay/stream resources but does not refresh activity. After incrementally decoding a valid request head, the adapter must call `StreamLease::accept_head` before dispatch. Only accepted heads refresh activity and their leases hold activity for both request and response lifetimes; dropping an accepted lease starts the next idle interval. Invalid or incomplete heads cannot keep a session alive. Adapters must hold the lease until both directions end or are cancelled and invoke `reap` periodically as well as at admission.

A local identity switch clears the table and invalidates outstanding device record keys and leases. Cloud login changes are not consulted for established sessions. Sessions, master secrets, and request keys are memory-only; the core does not persist device identities or browser state.

## Native HTTP adapter

The [`pontia-http` adapter](../crates/pontia-http/src/e2e.rs) accepts only `POST /e2e/v1/sessions` and `POST /e2e/v1/requests`, without query parameters, with `Content-Type: application/pontia-e2e`. Edge maps the matching public device paths to these internal paths without inspecting their bodies. Neither a local token nor an old browser Cookie grants access through this adapter to plaintext business paths.

Authenticated and validated bHTTP requests enter the same External API router as local Token requests, using an internal identity that network headers cannot supply. Local and E2E entrypoints share the External API's 2 MiB request-body policy; the E2E adapter fully authenticates a body within that business limit before dispatch. Response bodies are encoded and encrypted incrementally under consumer backpressure. Business statuses and allowlisted response headers are encrypted; successful outer transport uses HTTP 200. Unknown sessions use outer 409, capacity exhaustion 429, authentication/replay failures 401, malformed or truncated admission 400, and admission timeout 408. Errors after streaming begins terminate that stream, without implying business rollback.

The adapter bounds simultaneous pending and established transports to 64. Handshake bodies are limited to 1 KiB, and both handshake-body reading and business-head admission have a ten-second deadline. The shared core additionally limits handshake attempts and session/replay resources. A stream holds its activity lease through request validation and response completion; cancellation releases ownership without a detached forwarding task. Local identity replacement wakes outstanding streams and invalidates their keys. `pontiad` loads the private device identity from its private state file and schedules periodic idle-session reaping.

## Verification

Native checks:

```sh
cargo fmt -p pontia-e2e --check
cargo clippy -p pontia-e2e --all-targets -- -D warnings
cargo test -p pontia-e2e
```

WASM checks require the `wasm32-unknown-unknown` Rust target, Node, and a `wasm-bindgen-test-runner` matching the locked wasm-bindgen version:

```sh
cargo check -p pontia-e2e --target wasm32-unknown-unknown
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p pontia-e2e --target wasm32-unknown-unknown
```

Dashboard bindings are generated rather than tracked. Run `scripts/build-dashboard-wasm` with the locked `wasm-bindgen-cli` installed. Cloudflare CI passes `--install-bindgen`, then builds the Public Dashboard and deploys the resulting Worker assets.

Device-side WASM tests inject a monotonic clock; only native construction uses `std::time::Instant`. Production WASM exports only browser handles.

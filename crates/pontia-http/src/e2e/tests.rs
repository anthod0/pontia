use super::*;
use axum::{
    http::{HeaderMap, Method},
    routing::{get, post},
};
use ed25519_dalek::{Signer, SigningKey};
use pontia_e2e::{BrowserIdentity, BrowserSession, Capability, DeviceIdentity};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

const DEVICE: [u8; 16] = [7; 16];
fn signing_key() -> SigningKey {
    SigningKey::from_bytes(&[9; 32])
}
fn identity() -> DeviceIdentity {
    DeviceIdentity::from_private_bytes(DEVICE, 1, &[11; 32]).unwrap()
}
fn ingress(router: Router) -> E2eIngress {
    E2eIngress::new(
        DeviceSessions::new(identity(), signing_key().verifying_key()),
        router,
    )
}
fn outer(path: &str, body: Body) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, CONTENT_TYPE)
        .body(body)
        .unwrap()
}
async fn bytes(response: Response) -> Bytes {
    response.into_body().collect().await.unwrap().to_bytes()
}

async fn connect(ingress: &E2eIngress) -> BrowserSession {
    let browser = BrowserIdentity::generate().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut wire = vec![VERSION];
    wire.extend_from_slice(&DEVICE);
    wire.extend_from_slice(&1u64.to_be_bytes());
    wire.extend_from_slice(&browser.public_key());
    wire.extend_from_slice(&[13; 32]);
    wire.extend_from_slice(&now.to_be_bytes());
    wire.extend_from_slice(&(now + 60).to_be_bytes());
    wire.extend_from_slice(&[0; 64]);
    let capability = Capability::decode(&wire).unwrap();
    wire[105..].copy_from_slice(&signing_key().sign(&capability.signing_bytes()).to_bytes());
    let handshake = browser.start(&identity().public_key(), &wire).unwrap();
    let response = ingress
        .handle(outer(
            SESSIONS_PATH,
            Body::from(handshake.request_bytes().to_vec()),
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    handshake.confirm(&bytes(response).await).unwrap()
}

struct EncryptedRequest {
    wire: Vec<u8>,
    response: RecordDecoder,
}
fn encrypted_request(
    session: &BrowserSession,
    method: Method,
    path: &str,
    content: &[u8],
) -> EncryptedRequest {
    let (context, mut records, response) = session.request().unwrap();
    let (mut bhttp, head) = Encoder::new(&Head::Request {
        method,
        uri: path.parse().unwrap(),
        headers: HeaderMap::new(),
    })
    .unwrap();
    let mut wire = vec![VERSION];
    wire.extend_from_slice(&context.session_id);
    wire.extend_from_slice(&context.request_id);
    wire.extend_from_slice(&records.seal(&head).unwrap());
    for chunk in content.chunks(MAX_RECORD_PLAINTEXT) {
        let encoded = bhttp.content(chunk).unwrap();
        for record in encoded.chunks(MAX_RECORD_PLAINTEXT) {
            wire.extend_from_slice(&records.seal(record).unwrap());
        }
    }
    wire.extend_from_slice(&records.seal(&bhttp.finish().unwrap()).unwrap());
    wire.extend_from_slice(&records.finish().unwrap());
    EncryptedRequest { wire, response }
}
fn decrypt(mut records: RecordDecoder, bytes: &[u8]) -> (u16, HeaderMap, Vec<u8>) {
    let mut bhttp = Decoder::new(Kind::Response);
    let mut pending = bytes;
    let mut status = None;
    let mut headers = HeaderMap::new();
    let mut content = Vec::new();
    let mut ended = false;
    while !pending.is_empty() {
        let (used, plaintext) = records.feed(pending).unwrap();
        assert!(used > 0);
        pending = &pending[used..];
        if let Some(plaintext) = plaintext {
            let mut pending = plaintext.as_slice();
            while !pending.is_empty() {
                let (used, event) = bhttp.feed(pending).unwrap();
                assert!(used > 0);
                pending = &pending[used..];
                match event {
                    Some(Event::Head(Head::Response {
                        status: s,
                        headers: h,
                    })) => {
                        assert!(status.is_none());
                        status = Some(s);
                        headers = h;
                    }
                    Some(Event::Content(bytes)) => content.extend_from_slice(&bytes),
                    Some(Event::End) => ended = true,
                    Some(_) => panic!("request head in response"),
                    None => {}
                }
            }
        }
    }
    records.finish().unwrap();
    bhttp.finish().unwrap();
    assert!(ended);
    (status.unwrap(), headers, content)
}

#[tokio::test]
async fn binary_upload_and_business_error_are_encrypted_through_arbitrary_network_splits() {
    let ingress = ingress(Router::new().route(
        "/api/v1/upload",
        post(|body: Bytes| async move {
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                [
                    (header::CONTENT_TYPE, "application/octet-stream"),
                    (header::SERVER, "private-device"),
                ],
                body,
            )
        }),
    ));
    let session = connect(&ingress).await;
    let content: Vec<u8> = (0..50_000).map(|i| (i % 256) as u8).collect();
    let encrypted = encrypted_request(&session, Method::POST, "/api/v1/upload?raw=1", &content);
    let chunks: Vec<_> = encrypted
        .wire
        .chunks(7)
        .map(|chunk| Ok::<_, Error>(Bytes::copy_from_slice(chunk)))
        .collect();
    let response = ingress
        .handle(outer(
            REQUESTS_PATH,
            Body::from_stream(tokio_stream::iter(chunks)),
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    let wire = bytes(response).await;
    assert!(!wire.windows(128).any(|window| window == &content[..128]));
    let (status, headers, received) = decrypt(encrypted.response, &wire);
    assert_eq!(status, 422);
    assert_eq!(headers[header::CONTENT_TYPE], "application/octet-stream");
    assert!(!headers.contains_key(header::SERVER));
    assert_eq!(received, content);
}

#[tokio::test]
async fn replayed_ciphertext_cannot_execute_a_business_handler_twice() {
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = count.clone();
    let ingress = ingress(Router::new().route(
        "/api/v1/control",
        post(move || {
            let count = observed.clone();
            async move {
                count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                StatusCode::NO_CONTENT
            }
        }),
    ));
    let session = connect(&ingress).await;
    let encrypted = encrypted_request(&session, Method::POST, "/api/v1/control", &[]);
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire.clone())))
        .await;
    assert_eq!(decrypt(encrypted.response, &bytes(response).await).0, 204);
    let replay = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[tokio::test]
async fn invalid_first_record_cannot_reach_business_handler_or_poison_valid_retry() {
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = count.clone();
    let ingress = ingress(Router::new().route(
        "/api/v1/echo",
        get(move || {
            let count = observed.clone();
            async move {
                count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                "trusted response"
            }
        }),
    ));
    let session = connect(&ingress).await;
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/echo", &[]);
    let mut changed = encrypted.wire.clone();
    changed[75] ^= 1;
    let rejected = ingress
        .handle(outer(REQUESTS_PATH, Body::from(changed)))
        .await;
    assert_eq!(rejected.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 0);
    let accepted = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    let (status, _, content) = decrypt(encrypted.response, &bytes(accepted).await);
    assert_eq!(status, 200);
    assert_eq!(content, b"trusted response");
    assert_eq!(count.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[tokio::test]
async fn old_cookie_and_local_token_cannot_open_a_plaintext_business_path() {
    let ingress = ingress(Router::new().route("/api/v1/echo", post(|| async { "private" })));
    let mut request = outer("/api/v1/echo", Body::empty());
    request.headers_mut().insert(
        header::COOKIE,
        "pontia-browser-access=old-secret".parse().unwrap(),
    );
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, "Bearer local-token".parse().unwrap());
    assert_eq!(
        ingress.handle(request).await.status(),
        StatusCode::NOT_FOUND
    );
    let unknown = ingress
        .handle(outer(REQUESTS_PATH, Body::from(vec![1; 65 + 5 + 16])))
        .await;
    assert_ne!(unknown.status(), StatusCode::OK);
}

#[tokio::test]
async fn handler_can_respond_without_waiting_for_upload_eof() {
    let ingress =
        ingress(Router::new().route("/api/v1/reject", post(|| async { StatusCode::FORBIDDEN })));
    let session = connect(&ingress).await;
    let encrypted = encrypted_request(&session, Method::POST, "/api/v1/reject", &[]);
    let first_length = u32::from_be_bytes(encrypted.wire[65..69].try_into().unwrap()) as usize;
    let (sender, receiver) = mpsc::channel::<Result<Bytes, Error>>(1);
    sender
        .send(Ok(Bytes::copy_from_slice(
            &encrypted.wire[..65 + 5 + first_length],
        )))
        .await
        .unwrap();
    // The sender remains alive and has not supplied the bHTTP/record terminators.
    let response = tokio::time::timeout(
        Duration::from_secs(1),
        ingress.handle(outer(
            REQUESTS_PATH,
            Body::from_stream(ReceiverStream::new(receiver)),
        )),
    )
    .await
    .unwrap();
    let wire = tokio::time::timeout(Duration::from_secs(1), bytes(response))
        .await
        .unwrap();
    assert_eq!(decrypt(encrypted.response, &wire).0, 403);
    assert!(sender.is_closed());
}

#[tokio::test]
async fn truncated_upload_is_not_delivered_to_handler_as_successful_eof() {
    let ingress =
        ingress(Router::new().route("/api/v1/upload", post(|body: Bytes| async move { body })));
    let session = connect(&ingress).await;
    let mut encrypted =
        encrypted_request(&session, Method::POST, "/api/v1/upload", b"not complete");
    encrypted.wire.truncate(encrypted.wire.len() - 21); // missing authenticated final record
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn later_record_authentication_failure_cannot_be_hidden_by_a_streaming_handler() {
    let ingress = ingress(Router::new().route(
        "/api/v1/upload",
        post(|request: Request<Body>| async move {
            let mut body = request.into_body();
            Body::from_stream(async_stream::stream! {
                while let Some(frame) = body.frame().await {
                    // Deliberately swallow errors, as a handler might try to do.
                    let Ok(frame) = frame else { break; };
                    let Ok(bytes) = frame.into_data() else { break; };
                    yield Ok::<_, Error>(bytes);
                }
            })
        }),
    ));
    let session = connect(&ingress).await;
    let mut encrypted = encrypted_request(
        &session,
        Method::POST,
        "/api/v1/upload",
        b"modified content",
    );
    let head_size = u32::from_be_bytes(encrypted.wire[65..69].try_into().unwrap()) as usize;
    encrypted.wire[65 + 5 + head_size + 8] ^= 1;
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(response.status(), StatusCode::OK); // response can begin before upload consumption
    let mut body = response.into_body();
    assert!(body.frame().await.unwrap().is_ok()); // authenticated response head
    assert!(body.collect().await.is_err()); // never completes as a business response
}

#[tokio::test]
async fn identity_switch_wakes_authenticated_pending_head_admission() {
    let ingress = ingress(Router::new());
    let session = connect(&ingress).await;
    let (context, mut records, _) = session.request().unwrap();
    let mut first = vec![VERSION];
    first.extend_from_slice(&context.session_id);
    first.extend_from_slice(&context.request_id);
    first.extend_from_slice(&records.seal(&[2]).unwrap()); // partial valid request head
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let upload = async_stream::stream! {
        yield Ok::<_, Error>(Bytes::from(first));
        // Resuming after the first chunk proves admission is waiting for more head bytes.
        ready_tx.send(()).unwrap();
        std::future::pending::<()>().await;
    };
    let handler = ingress.clone();
    let pending = tokio::spawn(async move {
        handler
            .handle(outer(REQUESTS_PATH, Body::from_stream(upload)))
            .await
    });
    ready_rx.await.unwrap();
    ingress.replace_identity(identity());
    let response = tokio::time::timeout(Duration::from_secs(1), pending)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    // Newly admitted sessions must not inherit the preceding generation's notification.
    let current = connect(&ingress).await;
    let encrypted = encrypted_request(&current, Method::GET, "/api/v1/missing", &[]);
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(decrypt(encrypted.response, &bytes(response).await).0, 404);
}

#[tokio::test(start_paused = true)]
async fn unauthenticated_handshake_and_request_admission_have_deadlines() {
    let ingress = ingress(Router::new());
    for path in [SESSIONS_PATH, REQUESTS_PATH] {
        let upload = Body::from_stream(tokio_stream::pending::<Result<Bytes, Error>>());
        let response = ingress.handle(outer(path, upload)).await;
        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
    }
    // Timed-out readers release admission rather than filling the table indefinitely.
    assert_eq!(
        ingress
            .handle(outer(SESSIONS_PATH, Body::empty()))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn unauthenticated_pending_readers_share_a_bounded_admission_budget() {
    let ingress = ingress(Router::new());
    let (ready_tx, mut ready_rx) = mpsc::channel(MAX_STREAMS);
    let mut readers = Vec::new();
    for _ in 0..MAX_STREAMS {
        let ready_tx = ready_tx.clone();
        let handler = ingress.clone();
        let upload = async_stream::stream! {
            ready_tx.send(()).await.unwrap();
            std::future::pending::<()>().await;
            yield Ok::<_, Error>(Bytes::new());
        };
        readers.push(tokio::spawn(async move {
            handler
                .handle(outer(SESSIONS_PATH, Body::from_stream(upload)))
                .await
        }));
    }
    for _ in 0..MAX_STREAMS {
        ready_rx.recv().await.unwrap();
    }
    assert_eq!(
        ingress
            .handle(outer(SESSIONS_PATH, Body::empty()))
            .await
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    let cancelled = readers.pop().unwrap();
    cancelled.abort();
    assert!(cancelled.await.unwrap_err().is_cancelled());
    assert_eq!(
        ingress
            .handle(outer(SESSIONS_PATH, Body::empty()))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    for reader in readers {
        reader.abort();
        let _ = reader.await;
    }
}

#[tokio::test]
async fn authenticated_record_eof_cannot_replace_bhttp_termination() {
    let ingress =
        ingress(Router::new().route("/api/v1/upload", post(|body: Bytes| async move { body })));
    let session = connect(&ingress).await;
    let (context, mut records, _) = session.request().unwrap();
    let (_, head) = Encoder::new(&Head::Request {
        method: Method::POST,
        uri: "/api/v1/upload".parse().unwrap(),
        headers: HeaderMap::new(),
    })
    .unwrap();
    let mut wire = vec![VERSION];
    wire.extend_from_slice(&context.session_id);
    wire.extend_from_slice(&context.request_id);
    wire.extend_from_slice(&records.seal(&head).unwrap());
    wire.extend_from_slice(&records.finish().unwrap()); // no bHTTP content/trailer terminators
    assert_eq!(
        ingress
            .handle(outer(REQUESTS_PATH, Body::from(wire)))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn upload_ownership_keeps_a_session_active_after_response_completion() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let clock = Arc::new(AtomicU64::new(0));
    let monotonic = clock.clone();
    let held_upload = Arc::new(Mutex::new(None));
    let retained = held_upload.clone();
    let router = Router::new().route(
        "/api/v1/retain",
        post(move |request: Request<Body>| {
            *retained.lock().unwrap() = Some(request.into_body());
            async { StatusCode::NO_CONTENT }
        }),
    );
    let ingress = E2eIngress::new(
        DeviceSessions::with_clock(
            identity(),
            signing_key().verifying_key(),
            Arc::new(move || monotonic.load(Ordering::Relaxed)),
        ),
        router,
    );
    let session = connect(&ingress).await;
    let encrypted = encrypted_request(&session, Method::POST, "/api/v1/retain", &[]);
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(decrypt(encrypted.response, &bytes(response).await).0, 204);
    clock.store(pontia_e2e::IDLE_SECONDS * 2, Ordering::Relaxed);
    ingress.reap();
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/missing", &[]);
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(decrypt(encrypted.response, &bytes(response).await).0, 404);
    drop(held_upload.lock().unwrap().take());
    clock.store(pontia_e2e::IDLE_SECONDS * 3, Ordering::Relaxed);
    ingress.reap();
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/missing", &[]);
    assert_eq!(
        ingress
            .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
            .await
            .status(),
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn invalid_transport_and_oversized_records_are_rejected_before_business_dispatch() {
    let ingress = ingress(Router::new());
    for request in [
        Request::builder()
            .method("GET")
            .uri(SESSIONS_PATH)
            .header(header::CONTENT_TYPE, CONTENT_TYPE)
            .body(Body::empty())
            .unwrap(),
        outer("/e2e/v1/sessions?fallback=1", Body::empty()),
        outer(SESSIONS_PATH, Body::from(vec![0; MAX_HANDSHAKE_BYTES + 1])),
    ] {
        assert_ne!(ingress.handle(request).await.status(), StatusCode::OK);
    }
    let mut oversized = vec![VERSION];
    oversized.extend_from_slice(&[1; 64]);
    oversized.extend_from_slice(&u32::MAX.to_be_bytes());
    oversized.push(0);
    assert_eq!(
        ingress
            .handle(outer(REQUESTS_PATH, Body::from(oversized)))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
}

#[tokio::test]
async fn identity_switch_invalidates_a_live_stream_and_old_session() {
    let (sender, receiver) = mpsc::channel::<Result<Bytes, Error>>(1);
    let source = Arc::new(Mutex::new(Some(receiver)));
    let ingress = ingress(Router::new().route(
        "/api/v1/stream",
        get(move || {
            let receiver = source.lock().unwrap().take().unwrap();
            async move { Body::from_stream(ReceiverStream::new(receiver)) }
        }),
    ));
    let session = connect(&ingress).await;
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    let mut response = response.into_body();
    assert!(response.frame().await.unwrap().is_ok());
    ingress.replace_identity(DeviceIdentity::from_private_bytes(DEVICE, 2, &[17; 32]).unwrap());
    // Rotation must wake a stream even when its business producer is silent.
    let frame = tokio::time::timeout(Duration::from_secs(1), response.frame())
        .await
        .unwrap();
    assert!(frame.unwrap().is_err());
    drop(response);
    assert!(sender.is_closed());
    let old = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    assert_eq!(
        ingress
            .handle(outer(REQUESTS_PATH, Body::from(old.wire)))
            .await
            .status(),
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn cancelled_response_releases_stream_admission_without_waiting_for_producer() {
    let ingress = ingress(Router::new().route(
        "/api/v1/stream",
        get(|| async { Body::from_stream(tokio_stream::pending::<Result<Bytes, Error>>()) }),
    ));
    let session = connect(&ingress).await;
    let mut responses = Vec::new();
    for _ in 0..MAX_STREAMS {
        let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
        let response = ingress
            .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
            .await;
        assert_eq!(response.status(), StatusCode::OK);
        responses.push(response);
    }
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    let rejected = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(rejected.status(), StatusCode::TOO_MANY_REQUESTS);
    drop(responses.pop());
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    assert_eq!(
        ingress
            .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn slow_response_consumer_does_not_prebuffer_the_business_body() {
    let produced = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = produced.clone();
    let ingress = ingress(Router::new().route(
        "/api/v1/download",
        get(move || {
            let produced = observed.clone();
            async move {
                Body::from_stream(async_stream::stream! {
                    for _ in 0..100 {
                        produced.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        yield Ok::<_, Error>(Bytes::from(vec![42; MAX_RECORD_PLAINTEXT]));
                    }
                })
            }
        }),
    ));
    let session = connect(&ingress).await;
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/download", &[]);
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    let mut body = response.into_body();
    assert_eq!(produced.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert!(body.frame().await.unwrap().is_ok()); // encrypted head
    assert_eq!(produced.load(std::sync::atomic::Ordering::Relaxed), 0);
    assert!(body.frame().await.unwrap().is_ok()); // first content record
    assert_eq!(produced.load(std::sync::atomic::Ordering::Relaxed), 1);
    drop(body);
    assert_eq!(produced.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[tokio::test]
async fn active_stream_survives_four_hours_and_is_reaped_four_hours_after_cancellation() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let clock = Arc::new(AtomicU64::new(0));
    let monotonic = clock.clone();
    let sessions = DeviceSessions::with_clock(
        identity(),
        signing_key().verifying_key(),
        Arc::new(move || monotonic.load(Ordering::Relaxed)),
    );
    let ingress = E2eIngress::new(
        sessions,
        Router::new().route(
            "/api/v1/stream",
            get(|| async { Body::from_stream(tokio_stream::pending::<Result<Bytes, Error>>()) }),
        ),
    );
    let session = connect(&ingress).await;
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    clock.store(pontia_e2e::IDLE_SECONDS * 2, Ordering::Relaxed);
    ingress.reap();
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    let another = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(another.status(), StatusCode::OK);
    drop(response);
    drop(another);
    clock.store(pontia_e2e::IDLE_SECONDS * 3 - 1, Ordering::Relaxed);
    ingress.reap();
    // A valid authenticated stream at the boundary is still usable, and resets idle time on drop.
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    let response = ingress
        .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    drop(response);
    clock.store(pontia_e2e::IDLE_SECONDS * 4 - 1, Ordering::Relaxed);
    ingress.reap();
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    assert_eq!(
        ingress
            .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
            .await
            .status(),
        StatusCode::CONFLICT
    );
}

#[tokio::test]
async fn malformed_authenticated_head_does_not_keep_the_session_alive() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let clock = Arc::new(AtomicU64::new(0));
    let monotonic = clock.clone();
    let ingress = E2eIngress::new(
        DeviceSessions::with_clock(
            identity(),
            signing_key().verifying_key(),
            Arc::new(move || monotonic.load(Ordering::Relaxed)),
        ),
        Router::new(),
    );
    let session = connect(&ingress).await;
    clock.store(pontia_e2e::IDLE_SECONDS - 1, Ordering::Relaxed);
    let (context, mut records, _) = session.request().unwrap();
    let mut wire = vec![VERSION];
    wire.extend_from_slice(&context.session_id);
    wire.extend_from_slice(&context.request_id);
    wire.extend_from_slice(&records.seal(&[0]).unwrap()); // forbidden known-length bHTTP mode
    assert_eq!(
        ingress
            .handle(outer(REQUESTS_PATH, Body::from(wire)))
            .await
            .status(),
        StatusCode::BAD_REQUEST
    );
    clock.store(pontia_e2e::IDLE_SECONDS, Ordering::Relaxed);
    ingress.reap();
    let encrypted = encrypted_request(&session, Method::GET, "/api/v1/stream", &[]);
    assert_eq!(
        ingress
            .handle(outer(REQUESTS_PATH, Body::from(encrypted.wire)))
            .await
            .status(),
        StatusCode::CONFLICT
    );
}

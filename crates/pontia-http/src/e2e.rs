//! Network adapter for the shared browser/device protocol. Business requests are
//! dispatched only after authenticating records and validating the bHTTP head.
#[cfg(test)]
mod tests;
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use http_body_util::BodyExt;
use pontia_e2e::{
    DeviceSessions, Error, MAX_RECORD_PLAINTEXT, MAX_STREAMS, RecordDecoder, RecordEncoder,
    StreamLease, VERSION,
    bhttp::{Decoder, Encoder, Event, Head, Kind},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio_stream::StreamExt;
use tower::ServiceExt;

pub const SESSIONS_PATH: &str = "/e2e/v1/sessions";
pub const REQUESTS_PATH: &str = "/e2e/v1/requests";
pub const CONTENT_TYPE: &str = "application/pontia-e2e";
const ADMISSION_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_HANDSHAKE_BYTES: usize = 1024;

/// This identity cannot be supplied by a header or by a tunnel connection.
#[derive(Clone, Copy)]
pub(crate) struct AuthenticatedE2eRequest;

#[derive(Clone)]
pub struct E2eIngress {
    sessions: Arc<Mutex<DeviceSessions>>,
    business: Router,
    admissions: Arc<Semaphore>,
    invalidation: watch::Sender<()>,
}

impl E2eIngress {
    pub(crate) fn new(sessions: DeviceSessions, business: Router) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(sessions)),
            business,
            admissions: Arc::new(Semaphore::new(MAX_STREAMS)),
            invalidation: watch::channel(()).0,
        }
    }

    /// Maintenance uses the core's monotonic idle clock, not capability expiry.
    pub fn reap(&self) {
        self.sessions.lock().unwrap().reap();
    }

    /// Invalidates existing streams as well as sessions after a local key switch.
    pub fn replace_identity(&self, identity: pontia_e2e::DeviceIdentity) {
        // New admissions subscribe under this same lock, so they cannot receive
        // an invalidation belonging to the preceding identity generation.
        let mut sessions = self.sessions.lock().unwrap();
        sessions.replace_identity(identity);
        self.invalidation.send_replace(());
    }

    pub async fn handle(&self, request: Request<Body>) -> Response {
        if request.method() != axum::http::Method::POST
            || request.uri().query().is_some()
            || request
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                != Some(CONTENT_TYPE)
            || request.headers().contains_key(header::TRAILER)
        {
            return transport_error(Error::Protocol);
        }
        let path = request.uri().path();
        if path != SESSIONS_PATH && path != REQUESTS_PATH {
            return StatusCode::NOT_FOUND.into_response();
        }
        let Ok(permit) = self.admissions.clone().try_acquire_owned() else {
            return transport_error(Error::Capacity);
        };
        let session = path == SESSIONS_PATH;
        let mut input = Input::new(request.into_body());
        if session {
            let result = tokio::time::timeout(ADMISSION_TIMEOUT, self.handshake(&mut input)).await;
            drop(permit);
            return match result {
                Ok(Ok(bytes)) => ciphertext_response(Body::from(bytes)),
                Ok(Err(error)) => transport_error(error),
                Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
            };
        }
        let admitted = tokio::time::timeout(ADMISSION_TIMEOUT, self.admit(&mut input)).await;
        match admitted {
            Ok(Ok(admitted)) => self.dispatch(input, admitted, permit).await,
            Ok(Err(error)) => transport_error(error),
            Err(_) => StatusCode::REQUEST_TIMEOUT.into_response(),
        }
    }

    async fn handshake(&self, input: &mut Input) -> pontia_e2e::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        while let Some(chunk) = input.next().await? {
            if bytes.len() + chunk.len() > MAX_HANDSHAKE_BYTES {
                return Err(Error::Capacity);
            }
            bytes.extend_from_slice(&chunk);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::AuthorizationWindow)?
            .as_secs();
        self.sessions.lock().unwrap().handshake(&bytes, now)
    }

    async fn admit(&self, input: &mut Input) -> pontia_e2e::Result<Admitted> {
        let prefix = input.exact(65).await?;
        if prefix[0] != VERSION {
            return Err(Error::Protocol);
        }
        let first = input.record().await?;
        let ((plaintext, records, response, mut lease), mut invalidation) = {
            let mut sessions = self.sessions.lock().unwrap();
            let request = sessions.request(
                prefix[1..33].try_into().unwrap(),
                prefix[33..65].try_into().unwrap(),
                &first,
            )?;
            (request, self.invalidation.subscribe())
        };
        let mut decoder = DecodedInput {
            records,
            bhttp: Decoder::new(Kind::Request),
            pending: Bytes::from(plaintext),
        };
        loop {
            lease.check()?;
            if let Some(event) = decoder.event()? {
                match event {
                    Event::Head(head) => {
                        lease.accept_head(&head)?;
                        return Ok(Admitted {
                            head,
                            decoder,
                            response,
                            lease,
                            invalidation,
                        });
                    }
                    _ => return Err(Error::Protocol),
                }
            }
            tokio::select! {
                biased;
                _ = invalidation.changed() => Err(Error::UnknownSession),
                result = decoder.read_record(input) => result,
            }?;
        }
    }

    async fn dispatch(
        &self,
        mut input: Input,
        admitted: Admitted,
        permit: OwnedSemaphorePermit,
    ) -> Response {
        let Admitted {
            head,
            mut decoder,
            mut response,
            lease,
            mut invalidation,
        } = admitted;
        let Head::Request {
            method,
            uri,
            headers,
        } = head
        else {
            unreachable!()
        };
        let activity = Arc::new(Activity {
            lease,
            _permit: permit,
        });
        let upload_activity = activity.clone();
        let mut upload_invalidation = invalidation.clone();
        let (failure_sender, mut failure) = watch::channel(false);
        let upload_failure = failure_sender.clone();
        let upload = async_stream::try_stream! {
            loop {
                upload_activity.lease.check()?;
                if let Some(event) = decoder.event()? {
                    match event {
                        Event::Content(bytes) => yield Bytes::from(bytes),
                        Event::End => {},
                        Event::Head(_) => Err(Error::Protocol)?,
                    }
                    continue;
                }
                let available = tokio::select! {
                    biased;
                    _ = upload_invalidation.changed() => Err(Error::UnknownSession),
                    available = input.peek() => available,
                }?;
                if available.is_none() {
                    decoder.records.finish()?;
                    decoder.bhttp.finish()?;
                    break;
                }
                tokio::select! {
                    biased;
                    _ = upload_invalidation.changed() => Err(Error::UnknownSession),
                    result = decoder.read_record(&mut input) => result,
                }?;
            }
            Ok::<(), Error>(())?;
        };
        // The handler may convert a request-body error into an ordinary response.
        // Record/protocol failures instead invalidate the entire transport, even
        // when the handler catches the error or has already started responding.
        let upload = upload.map(move |result: Result<Bytes, Error>| {
            if result.is_err() {
                upload_failure.send_replace(true);
            }
            result
        });
        let mut request = Request::new(body_stream(upload));
        *request.method_mut() = method;
        *request.uri_mut() = uri;
        *request.headers_mut() = headers;
        request.extensions_mut().insert(AuthenticatedE2eRequest);
        if let Err(error) = activity.lease.check() {
            return transport_error(error);
        }
        let result = tokio::select! {
            biased;
            _ = invalidation.changed() => return transport_error(Error::UnknownSession),
            _ = failure.changed() => return transport_error(Error::Protocol),
            result = self.business.clone().oneshot(request) => result.expect("router is infallible"),
        };
        if *failure.borrow() {
            return transport_error(Error::Protocol);
        }
        let (parts, body) = result.into_parts();
        let headers = pontia_e2e::bhttp::response_headers(&parts.headers);
        let encoded = Encoder::new(&Head::Response {
            status: parts.status.as_u16(),
            headers,
        });
        let Ok((mut bhttp, head)) = encoded else {
            return transport_error(Error::Protocol);
        };
        let stream = async_stream::try_stream! {
            // Keep the channel open if the handler has finished/dropped upload.
            let _failure_sender = failure_sender;
            activity.lease.check()?;
            if *failure.borrow() { Err(Error::Protocol)?; }
            yield Bytes::from(response.seal(&head)?);
            let mut body = body;
            while let Some(frame) = tokio::select! {
                biased;
                _ = invalidation.changed() => Err(Error::UnknownSession),
                _ = failure.changed() => Err(Error::Protocol),
                frame = body.frame() => Ok(frame),
            }? {
                if *failure.borrow() { Err(Error::Protocol)?; }
                let frame = frame.map_err(|_| Error::Truncated)?;
                let bytes = frame.into_data().map_err(|_| Error::Protocol)?;
                for chunk in bytes.chunks(MAX_RECORD_PLAINTEXT) {
                    activity.lease.check()?;
                    if *failure.borrow() { Err(Error::Protocol)?; }
                    let encoded = bhttp.content(chunk)?;
                    for record in encoded.chunks(MAX_RECORD_PLAINTEXT) {
                        yield Bytes::from(response.seal(record)?);
                    }
                }
            }
            if *failure.borrow() { Err(Error::Protocol)?; }
            yield Bytes::from(response.seal(&bhttp.finish()?)?);
            if *failure.borrow() { Err(Error::Protocol)?; }
            yield Bytes::from(response.finish()?);
            Ok::<(), Error>(())?;
        };
        ciphertext_response(body_stream(stream))
    }
}

struct Activity {
    lease: StreamLease,
    _permit: OwnedSemaphorePermit,
}
struct Admitted {
    head: Head,
    decoder: DecodedInput,
    response: RecordEncoder,
    lease: StreamLease,
    invalidation: watch::Receiver<()>,
}

struct DecodedInput {
    records: RecordDecoder,
    bhttp: Decoder,
    pending: Bytes,
}
impl DecodedInput {
    fn event(&mut self) -> pontia_e2e::Result<Option<Event>> {
        while !self.pending.is_empty() {
            let (used, event) = self.bhttp.feed(&self.pending)?;
            self.pending = self.pending.slice(used..);
            if event.is_some() {
                return Ok(event);
            }
            if used == 0 {
                return Err(Error::Protocol);
            }
        }
        Ok(None)
    }
    async fn read_record(&mut self, input: &mut Input) -> pontia_e2e::Result<()> {
        let bytes = input.record().await?;
        let (used, plaintext) = self.records.feed(&bytes)?;
        if used != bytes.len() {
            return Err(Error::Protocol);
        }
        self.pending = Bytes::from(plaintext.unwrap_or_default());
        Ok(())
    }
}

struct Input {
    body: Body,
    pending: Bytes,
}
impl Input {
    fn new(body: Body) -> Self {
        Self {
            body,
            pending: Bytes::new(),
        }
    }
    async fn peek(&mut self) -> pontia_e2e::Result<Option<()>> {
        while self.pending.is_empty() {
            let Some(frame) = self.body.frame().await else {
                return Ok(None);
            };
            self.pending = frame
                .map_err(|_| Error::Truncated)?
                .into_data()
                .map_err(|_| Error::Protocol)?;
        }
        Ok(Some(()))
    }
    async fn next(&mut self) -> pontia_e2e::Result<Option<Bytes>> {
        if self.peek().await?.is_none() {
            return Ok(None);
        }
        Ok(Some(std::mem::take(&mut self.pending)))
    }
    async fn exact(&mut self, size: usize) -> pontia_e2e::Result<Vec<u8>> {
        let mut bytes = Vec::with_capacity(size);
        while bytes.len() < size {
            if self.peek().await?.is_none() {
                return Err(Error::Truncated);
            }
            let take = (size - bytes.len()).min(self.pending.len());
            bytes.extend_from_slice(&self.pending.split_to(take));
        }
        Ok(bytes)
    }
    async fn record(&mut self) -> pontia_e2e::Result<Vec<u8>> {
        let mut prefix = self.exact(5).await?;
        let size = u32::from_be_bytes(prefix[..4].try_into().unwrap()) as usize;
        if !(16..=MAX_RECORD_PLAINTEXT + 16).contains(&size)
            || prefix[4] > 1
            || (prefix[4] == 1 && size != 16)
            || (prefix[4] == 0 && size == 16)
        {
            return Err(Error::Protocol);
        }
        prefix.extend_from_slice(&self.exact(size).await?);
        Ok(prefix)
    }
}

fn body_stream(
    stream: impl tokio_stream::Stream<Item = Result<Bytes, Error>> + Send + 'static,
) -> Body {
    Body::from_stream(stream)
}

fn ciphertext_response(body: Body) -> Response {
    (
        [
            (header::CONTENT_TYPE, CONTENT_TYPE),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response()
}
fn transport_error(error: Error) -> Response {
    let (status, code) = match error {
        Error::UnknownSession => (StatusCode::CONFLICT, "e2e_unknown_session"),
        Error::Capacity => (StatusCode::TOO_MANY_REQUESTS, "e2e_capacity"),
        Error::Authentication | Error::AuthorizationWindow | Error::Replay => {
            (StatusCode::UNAUTHORIZED, "e2e_authentication")
        }
        _ => (StatusCode::BAD_REQUEST, "e2e_protocol"),
    };
    (status, [(header::CACHE_CONTROL, "no-store")], code).into_response()
}

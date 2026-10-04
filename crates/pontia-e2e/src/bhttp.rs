//! Restricted RFC 9292 indeterminate-length profile. Heads use the bhttp crate;
//! the incremental framing guard bounds allocations before its parser is invoked.
use ::bhttp::{ControlData, Message, Mode};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};
use std::io::Cursor;

use crate::{Error, MAX_RECORD_PLAINTEXT, Result};

pub const MAX_HEAD_BYTES: usize = 16 * 1024;
pub const MAX_CONTENT_CHUNK: u64 = 1024 * 1024;
const SCHEME: &[u8] = b"https";
const AUTHORITY: &[u8] = b"pontia-device";
const REQUEST_HEADERS: &[&str] = &["accept", "content-type", "idempotency-key", "last-event-id"];
const RESPONSE_HEADERS: &[&str] = &[
    "content-type",
    "cache-control",
    "etag",
    "last-modified",
    "content-disposition",
    "retry-after",
    "allow",
];

#[derive(Clone, Copy)]
pub enum Kind {
    Request,
    Response,
}

pub enum Head {
    Request {
        method: Method,
        uri: Uri,
        headers: HeaderMap,
    },
    Response {
        status: u16,
        headers: HeaderMap,
    },
}

pub enum Event {
    Head(Head),
    Content(Vec<u8>),
    End,
}

pub fn canonical_api_uri(uri: &Uri) -> bool {
    let path = uri.path();
    if uri.scheme().is_some()
        || uri.authority().is_some()
        || !path.starts_with("/api/v1/")
        || path.contains('\\')
    {
        return false;
    }
    path[1..].split('/').all(|segment| {
        if segment.is_empty() || segment == "." || segment == ".." {
            return false;
        }
        let bytes = segment.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'%' {
                index += 1;
                continue;
            }
            if index + 2 >= bytes.len() {
                return false;
            }
            let upper_hex = |b| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            };
            let (Some(high), Some(low)) =
                (upper_hex(bytes[index + 1]), upper_hex(bytes[index + 2]))
            else {
                return false;
            };
            let value: u8 = high * 16 + low;
            if value == b'/'
                || value == b'\\'
                || value.is_ascii_alphanumeric()
                || b"-._~".contains(&value)
            {
                return false;
            }
            index += 3;
        }
        true
    })
}

fn allowed_method(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

pub(crate) fn validate_head(head: &Head) -> Result<()> {
    let (headers, allowed) = match head {
        Head::Request {
            method,
            uri,
            headers,
        } => {
            if !allowed_method(method) || !canonical_api_uri(uri) {
                return Err(Error::Protocol);
            }
            (headers, REQUEST_HEADERS)
        }
        Head::Response { status, headers } => {
            if !(200..=599).contains(status) {
                return Err(Error::Protocol);
            }
            (headers, RESPONSE_HEADERS)
        }
    };
    let mut size = 0;
    for (name, value) in headers {
        if !allowed.contains(&name.as_str()) {
            return Err(Error::Protocol);
        }
        size += name.as_str().len() + value.as_bytes().len() + 16;
        if size > MAX_HEAD_BYTES {
            return Err(Error::Capacity);
        }
    }
    Ok(())
}

/// Encodes only the head; content may be produced indefinitely with bounded chunks.
/// No body bytes are retained by the encoder.
pub struct Encoder {
    closed: bool,
}
impl Encoder {
    pub fn new(head: &Head) -> Result<(Self, Vec<u8>)> {
        validate_head(head)?;
        let (mut message, headers) = match head {
            Head::Request {
                method,
                uri,
                headers,
            } => (
                Message::request(
                    method.as_str().as_bytes().to_vec(),
                    SCHEME.to_vec(),
                    AUTHORITY.to_vec(),
                    uri.to_string().into_bytes(),
                ),
                headers,
            ),
            Head::Response { status, headers } => (
                Message::response((*status).try_into().map_err(|_| Error::Protocol)?),
                headers,
            ),
        };
        for (name, value) in headers {
            message.put_header(name.as_str().as_bytes(), value.as_bytes());
        }
        let mut bytes = Vec::new();
        message
            .write_bhttp(Mode::IndeterminateLength, &mut bytes)
            .map_err(|_| Error::Protocol)?;
        // The empty content and empty trailer each encode as a single zero.
        bytes.truncate(bytes.len() - 2);
        if bytes.len() > MAX_HEAD_BYTES {
            return Err(Error::Capacity);
        }
        Ok((Self { closed: false }, bytes))
    }

    pub fn content(&mut self, bytes: &[u8]) -> Result<Vec<u8>> {
        if self.closed {
            return Err(Error::Closed);
        }
        if bytes.is_empty() || bytes.len() > MAX_RECORD_PLAINTEXT {
            self.closed = true;
            return Err(Error::Capacity);
        }
        let mut output = encode_varint(bytes.len() as u64);
        output.extend_from_slice(bytes);
        Ok(output)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>> {
        if self.closed {
            return Err(Error::Closed);
        }
        self.closed = true;
        Ok(vec![0, 0])
    }
}

fn encode_varint(value: u64) -> Vec<u8> {
    if value < 64 {
        vec![value as u8]
    } else if value < 16384 {
        ((value as u16) | 0x4000).to_be_bytes().to_vec()
    } else if value < 1 << 30 {
        ((value as u32) | 0x8000_0000).to_be_bytes().to_vec()
    } else {
        (value | 0xc000_0000_0000_0000).to_be_bytes().to_vec()
    }
}

fn varint(input: &[u8]) -> Option<(u64, usize)> {
    let first = *input.first()?;
    let len = 1 << (first >> 6);
    if input.len() < len {
        return None;
    }
    let mut value = u64::from(first & 0x3f);
    for byte in &input[1..len] {
        value = (value << 8) | u64::from(*byte);
    }
    Some((value, len))
}

fn vector_end(input: &[u8], offset: usize) -> Result<Option<usize>> {
    let Some((len, prefix)) = varint(&input[offset..]) else {
        return Ok(None);
    };
    if len > MAX_HEAD_BYTES as u64 {
        return Err(Error::Capacity);
    }
    let end = offset + prefix + len as usize;
    if end > MAX_HEAD_BYTES {
        return Err(Error::Capacity);
    }
    Ok((end <= input.len()).then_some(end))
}

// Preflight lengths before bhttp is permitted to allocate any vectors.
fn head_end(input: &[u8], kind: Kind) -> Result<Option<usize>> {
    let Some((mode, mut offset)) = varint(input) else {
        return Ok(None);
    };
    match kind {
        Kind::Request => {
            if mode != 2 {
                return Err(Error::Protocol);
            }
            for _ in 0..4 {
                let Some(end) = vector_end(input, offset)? else {
                    return Ok(None);
                };
                offset = end;
            }
        }
        Kind::Response => {
            if mode != 3 {
                return Err(Error::Protocol);
            }
            let Some((status, len)) = varint(&input[offset..]) else {
                return Ok(None);
            };
            if !(200..=599).contains(&status) {
                return Err(Error::Protocol);
            }
            offset += len;
        }
    }
    loop {
        let Some((length, prefix)) = varint(&input[offset..]) else {
            return Ok(None);
        };
        if length == 0 {
            return Ok(Some(offset + prefix));
        }
        let Some(end) = vector_end(input, offset)? else {
            return Ok(None);
        };
        let Some(end) = vector_end(input, end)? else {
            return Ok(None);
        };
        offset = end;
    }
}

fn decode_head(mut bytes: Vec<u8>) -> Result<Head> {
    bytes.extend_from_slice(&[0, 0]);
    let message = Message::read_bhttp(&mut Cursor::new(bytes)).map_err(|_| Error::Protocol)?;
    let mut headers = HeaderMap::new();
    for field in message.header().iter() {
        // Only lowercase, allowlisted header names are accepted by the profile.
        let name = HeaderName::from_bytes(field.name()).map_err(|_| Error::Protocol)?;
        if name.as_str().as_bytes() != field.name() {
            return Err(Error::Protocol);
        }
        let value = HeaderValue::from_bytes(field.value()).map_err(|_| Error::Protocol)?;
        headers.append(name, value);
    }
    let head = match message.control() {
        ControlData::Request {
            method,
            scheme,
            authority,
            path,
        } => {
            if scheme != SCHEME || authority != AUTHORITY {
                return Err(Error::Protocol);
            }
            Head::Request {
                method: Method::from_bytes(method).map_err(|_| Error::Protocol)?,
                uri: std::str::from_utf8(path)
                    .map_err(|_| Error::Protocol)?
                    .parse()
                    .map_err(|_| Error::Protocol)?,
                headers,
            }
        }
        ControlData::Response(status) => Head::Response {
            status: status.code(),
            headers,
        },
    };
    validate_head(&head)?;
    Ok(head)
}

enum State {
    Head,
    Length,
    Content(u64),
    Trailer,
    Done,
    Failed,
}
pub struct Decoder {
    kind: Kind,
    state: State,
    buffer: Vec<u8>,
}
impl Decoder {
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            state: State::Head,
            buffer: Vec::new(),
        }
    }

    /// Returns the consumed prefix and at most one event. Retained bytes are bounded
    /// to a head or a varint; content is delivered immediately under caller backpressure.
    pub fn feed(&mut self, input: &[u8]) -> Result<(usize, Option<Event>)> {
        let result = self.feed_inner(input);
        if result.is_err() {
            self.state = State::Failed;
            self.buffer.clear();
        }
        result
    }

    fn feed_inner(&mut self, input: &[u8]) -> Result<(usize, Option<Event>)> {
        match self.state {
            State::Failed => Err(Error::Closed),
            State::Done => {
                if input.is_empty() {
                    Ok((0, None))
                } else {
                    Err(Error::Protocol)
                }
            }
            State::Head => {
                let previous = self.buffer.len();
                let take = (MAX_HEAD_BYTES - previous).min(input.len());
                self.buffer.extend_from_slice(&input[..take]);
                if let Some(end) = head_end(&self.buffer, self.kind)? {
                    self.buffer.truncate(end);
                    let head = decode_head(std::mem::take(&mut self.buffer))?;
                    self.state = State::Length;
                    Ok((end - previous, Some(Event::Head(head))))
                } else if self.buffer.len() == MAX_HEAD_BYTES {
                    Err(Error::Capacity)
                } else {
                    Ok((take, None))
                }
            }
            State::Length | State::Trailer => {
                let mut consumed = 0;
                while consumed < input.len() {
                    self.buffer.push(input[consumed]);
                    consumed += 1;
                    if let Some((length, _)) = varint(&self.buffer) {
                        self.buffer.clear();
                        if matches!(self.state, State::Trailer) {
                            if length != 0 {
                                return Err(Error::Protocol);
                            }
                            self.state = State::Done;
                            return Ok((consumed, Some(Event::End)));
                        }
                        if length > MAX_CONTENT_CHUNK {
                            return Err(Error::Capacity);
                        }
                        self.state = if length == 0 {
                            State::Trailer
                        } else {
                            State::Content(length)
                        };
                        return Ok((consumed, None));
                    }
                }
                Ok((consumed, None))
            }
            State::Content(remaining) => {
                let take = (remaining as usize)
                    .min(input.len())
                    .min(MAX_RECORD_PLAINTEXT);
                if take == 0 {
                    return Ok((0, None));
                }
                let next = remaining - take as u64;
                self.state = if next == 0 {
                    State::Length
                } else {
                    State::Content(next)
                };
                Ok((take, Some(Event::Content(input[..take].to_vec()))))
            }
        }
    }

    pub fn finish(&mut self) -> Result<()> {
        if matches!(self.state, State::Done) {
            Ok(())
        } else {
            self.state = State::Failed;
            self.buffer.clear();
            Err(Error::Truncated)
        }
    }
}

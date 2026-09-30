use axum::http::{HeaderMap, HeaderName, Method, Uri};

pub const SUBPROTOCOL: &str = "pontia-tunnel-h2-v1";
pub const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 64 * 1024;
pub const ADAPTER_BUFFER_BYTES: usize = 64 * 1024;
pub const MAX_WRITER_BUFFER_BYTES: usize = 256 * 1024;
pub const MAX_CONCURRENT_STREAMS: usize = 64;
pub const STREAM_WINDOW_BYTES: u32 = 256 * 1024;
pub const CONNECTION_WINDOW_BYTES: u32 = 4 * 1024 * 1024;
pub const MAX_FRAME_BYTES: u32 = 16 * 1024;
pub const MAX_HEADER_LIST_BYTES: u32 = 16 * 1024;

const REQUEST_HEADERS: &[&str] = &["accept", "content-type", "idempotency-key", "last-event-id"];
pub const CORS_REQUEST_HEADERS: &str = "Accept, Content-Type, Idempotency-Key, Last-Event-ID";
const RESPONSE_HEADERS: &[&str] = &[
    "content-type",
    "cache-control",
    "etag",
    "last-modified",
    "content-disposition",
    "retry-after",
    "allow",
];

pub fn method_allowed(method: &Method) -> bool {
    matches!(
        *method,
        Method::GET | Method::HEAD | Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    )
}

pub fn canonical_api_uri(uri: &Uri) -> bool {
    let path = uri.path();
    if !path.starts_with("/api/v1/") || path.contains('\\') {
        return false;
    }
    path[1..].split('/').all(canonical_segment)
}

fn canonical_segment(segment: &str) -> bool {
    if segment.is_empty() || segment == "." || segment == ".." {
        return false;
    }
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return false;
        }
        let Some(high) = upper_hex(bytes[index + 1]) else {
            return false;
        };
        let Some(low) = upper_hex(bytes[index + 2]) else {
            return false;
        };
        let value = high * 16 + low;
        if value == b'/'
            || value == b'\\'
            || value.is_ascii_alphanumeric()
            || b"-._~".contains(&value)
        {
            return false;
        }
        decoded.push(value);
        index += 3;
    }
    decoded.as_slice() != b"." && decoded.as_slice() != b".."
}

fn upper_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub fn request_header_allowed(name: &HeaderName) -> bool {
    REQUEST_HEADERS.contains(&name.as_str())
}

pub fn request_headers(headers: &HeaderMap) -> HeaderMap {
    allowlisted_headers(headers, REQUEST_HEADERS)
}

pub fn response_headers(headers: &HeaderMap) -> HeaderMap {
    allowlisted_headers(headers, RESPONSE_HEADERS)
}

fn allowlisted_headers(headers: &HeaderMap, allowed: &[&'static str]) -> HeaderMap {
    let mut filtered = HeaderMap::new();
    for name in allowed {
        for value in headers.get_all(*name).iter() {
            filtered.append(HeaderName::from_static(name), value.clone());
        }
    }
    filtered
}

#[cfg(test)]
mod tests {
    use super::canonical_api_uri;

    #[test]
    fn accepts_only_canonical_external_api_paths() {
        for valid in [
            "/api/v1/sessions",
            "/api/v1/sessions/abc?cursor=a%2Fb",
            "/api/v1/search?q=..%2Fprivate",
        ] {
            assert!(canonical_api_uri(&valid.parse().unwrap()), "{valid}");
        }
        for invalid in [
            "/api/v1/",
            "/api/v1//sessions",
            "/api/v1/./sessions",
            "/api/v1/%2E%2E/private",
            "/api/v1/a%2Fb",
            "/api/v1/a%5Cb",
            "/api/v1/%73essions",
            "/api/v1/a%2fb",
            "/dashboard",
        ] {
            assert!(!canonical_api_uri(&invalid.parse().unwrap()), "{invalid}");
        }
    }
}

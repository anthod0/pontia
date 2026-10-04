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

pub fn canonical_tunnel_uri(uri: &Uri) -> bool {
    uri.query().is_none() && matches!(uri.path(), "/e2e/v1/sessions" | "/e2e/v1/requests")
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
    use super::canonical_tunnel_uri;

    #[test]
    fn accepts_only_fixed_e2e_transport_paths() {
        for valid in ["/e2e/v1/sessions", "/e2e/v1/requests"] {
            assert!(canonical_tunnel_uri(&valid.parse().unwrap()), "{valid}");
        }
        for invalid in [
            "/e2e/v1/sessions?fallback=1",
            "/e2e/v1/requests/extra",
            "/api/v1/sessions",
            "/dashboard",
        ] {
            assert!(
                !canonical_tunnel_uri(&invalid.parse().unwrap()),
                "{invalid}"
            );
        }
    }
}

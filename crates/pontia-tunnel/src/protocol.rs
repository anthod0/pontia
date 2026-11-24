use axum::http::{HeaderMap, HeaderName, Method, Uri};

pub const SUBPROTOCOL: &str = "pontia-tunnel-h2-v1";
pub const MAX_WEBSOCKET_MESSAGE_BYTES: usize = 64 * 1024;
pub const ADAPTER_BUFFER_BYTES: usize = 64 * 1024;
pub const MAX_WRITER_BUFFER_BYTES: usize = 256 * 1024;
pub const MAX_CONCURRENT_STREAMS: usize = 256;
pub const STREAM_WINDOW_BYTES: u32 = 1024 * 1024;
pub const CONNECTION_WINDOW_BYTES: u32 = 16 * 1024 * 1024;
pub const MAX_FRAME_BYTES: u32 = 16 * 1024;
pub const MAX_HEADER_LIST_BYTES: u32 = 32 * 1024;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelRoute {
    E2e,
    PreviewHttp { port: u16 },
    PreviewWs { port: u16 },
}

impl TunnelRoute {
    pub fn parse(uri: &Uri) -> Option<Self> {
        if canonical_tunnel_uri(uri) {
            return Some(Self::E2e);
        }
        let (kind, target) = uri.path().strip_prefix("/preview/v1/")?.split_once('/')?;
        let (port, path) = target.split_once('/')?;
        if port.starts_with('0')
            || !port.bytes().all(|b| b.is_ascii_digit())
            || !valid_origin_component(path)
            || uri
                .query()
                .is_some_and(|query| !valid_origin_component(query))
        {
            return None;
        }
        let port = port.parse().ok()?;
        match kind {
            "http" => Some(Self::PreviewHttp { port }),
            "ws" => Some(Self::PreviewWs { port }),
            _ => None,
        }
    }

    pub(crate) fn allows(self, method: &Method) -> bool {
        match self {
            Self::PreviewHttp { .. } => method_allowed(method) || method == Method::OPTIONS,
            Self::E2e | Self::PreviewWs { .. } => method == Method::POST,
        }
    }
}

// RFC 3986 pchar, plus the path and query delimiters already separated by Uri.
fn valid_origin_component(component: &str) -> bool {
    let mut bytes = component.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            if !(0..2).all(|_| bytes.next().is_some_and(|b| b.is_ascii_hexdigit())) {
                return false;
            }
        } else if !(byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/?".contains(&byte)) {
            return false;
        }
    }
    true
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HeaderDirection {
    Request,
    Response,
}

/// Filters the browser or local application's headers before adding trusted tunnel metadata.
pub fn preview_headers(
    headers: &HeaderMap,
    route: TunnelRoute,
    direction: HeaderDirection,
) -> HeaderMap {
    filter_headers(headers, route, direction, false)
}

pub(crate) fn tunnel_headers(
    headers: &HeaderMap,
    route: TunnelRoute,
    direction: HeaderDirection,
) -> HeaderMap {
    filter_headers(headers, route, direction, true)
}

fn filter_headers(
    headers: &HeaderMap,
    route: TunnelRoute,
    direction: HeaderDirection,
    trusted: bool,
) -> HeaderMap {
    if route == TunnelRoute::E2e {
        return match direction {
            HeaderDirection::Request => request_headers(headers),
            HeaderDirection::Response => response_headers(headers),
        };
    }
    let connection_options: Vec<_> = headers
        .get_all("connection")
        .iter()
        .flat_map(|value| value.as_bytes().split(|&b| b == b','))
        .filter_map(|token| HeaderName::from_bytes(token.trim_ascii()).ok())
        .collect();
    let mut filtered = HeaderMap::new();
    for (name, value) in headers {
        let key = name.as_str();
        if connection_options.contains(name)
            || matches!(
                key,
                "connection"
                    | "keep-alive"
                    | "proxy-connection"
                    | "proxy-authenticate"
                    | "proxy-authorization"
                    | "te"
                    | "trailer"
                    | "trailers"
                    | "transfer-encoding"
                    | "upgrade"
            )
        {
            continue;
        }
        let allowed = if key.starts_with("x-pontia-") {
            trusted
                && matches!(
                    (direction, key),
                    (HeaderDirection::Request, "x-pontia-preview-host")
                        | (HeaderDirection::Response, "x-pontia-preview-error")
                )
        } else if matches!(route, TunnelRoute::PreviewWs { .. }) {
            key == "sec-websocket-protocol"
                || (direction == HeaderDirection::Request
                    && matches!(key, "cookie" | "origin" | "user-agent" | "accept-language"))
        } else {
            true
        };
        if allowed {
            filtered.append(name.clone(), value.clone());
        }
    }
    filtered
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
    use super::*;

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

    #[test]
    fn classifies_only_canonical_port_segments() {
        for (target, route) in [
            ("/preview/v1/http/1/", TunnelRoute::PreviewHttp { port: 1 }),
            (
                "/preview/v1/ws/65535/%2F/a?x=/b",
                TunnelRoute::PreviewWs { port: 65535 },
            ),
            ("/e2e/v1/requests", TunnelRoute::E2e),
        ] {
            assert_eq!(TunnelRoute::parse(&target.parse().unwrap()), Some(route));
        }
        for target in [
            "/preview/v1/ws/0/",
            "/preview/v1/http/01/",
            "/preview/v1/http/+1/",
            "/preview/v1/ws/65536/",
            "/preview/v1/ws/%31/",
            "/preview/v1/ws/1?x=/",
            "/preview/v1/ws//",
            "/preview/v1/http/1",
            "/preview/v1/tcp/1/",
            "/preview/v1/http/localhost:80/",
            "/preview/v1/http/80/%",
            "/preview/v1/http/80/?x=%GG",
            "/preview/v1/http/80/a\\b",
            "/e2e/v1/sessions?x",
        ] {
            assert_eq!(
                TunnelRoute::parse(&target.parse().unwrap()),
                None,
                "{target}"
            );
        }
    }

    #[test]
    fn filters_hop_headers_and_preserves_repeated_end_to_end_headers() {
        let mut headers = HeaderMap::new();
        for (name, value) in [
            ("connection", "Foo, UPgrade"),
            ("connection", "bar"),
            ("foo", "1"),
            ("bar", "2"),
            ("upgrade", "websocket"),
            ("trailer", "etag"),
            ("x-pontia-fake", "1"),
            ("x-pontia-preview-host", "host"),
            ("x-pontia-preview-error", "error"),
            ("set-cookie", "a=1"),
            ("set-cookie", "b=2"),
        ] {
            headers.append(name, value.parse().unwrap());
        }
        let http = TunnelRoute::PreviewHttp { port: 80 };
        let external = preview_headers(&headers, http, HeaderDirection::Response);
        assert_eq!(external.len(), 2);
        assert_eq!(
            external.get_all("set-cookie").iter().collect::<Vec<_>>(),
            ["a=1", "b=2"]
        );
        let internal = tunnel_headers(&headers, http, HeaderDirection::Response);
        assert_eq!(internal.len(), 3);
        assert_eq!(internal["x-pontia-preview-error"], "error");
        let ws = TunnelRoute::PreviewWs { port: 80 };
        headers.insert("sec-websocket-protocol", "vite-hmr".parse().unwrap());
        headers.insert("cookie", "app=1".parse().unwrap());
        headers.insert("authorization", "secret".parse().unwrap());
        let request = tunnel_headers(&headers, ws, HeaderDirection::Request);
        assert_eq!(request.len(), 3);
        assert_eq!(request["sec-websocket-protocol"], "vite-hmr");
        assert_eq!(request["cookie"], "app=1");
        assert_eq!(request["x-pontia-preview-host"], "host");
        let response = preview_headers(&headers, ws, HeaderDirection::Response);
        assert_eq!(response.len(), 1);
        assert_eq!(response["sec-websocket-protocol"], "vite-hmr");
    }
}

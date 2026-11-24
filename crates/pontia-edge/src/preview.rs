mod auth;
mod ws;

use axum::{
    Router,
    body::Body,
    extract::{FromRequestParts, Request, State, WebSocketUpgrade},
    http::{Method, StatusCode, Uri, header, uri::Authority},
    response::{Html, IntoResponse, Response},
};
use futures_util::{StreamExt, stream};
use pontia_tunnel::{
    TunnelError,
    protocol::{HeaderDirection, TunnelRoute, preview_headers},
};
use tower::ServiceExt;
use uuid::Uuid;

use crate::{Edge, access_token::PreviewTokenVerifier};

#[derive(Clone, Copy)]
struct Target {
    device: Uuid,
    port: u16,
}

impl Target {
    fn parse(label: &str) -> Option<Self> {
        let (port, hex) = label.split_once('-')?;
        if port.starts_with('0')
            || !port.bytes().all(|b| b.is_ascii_digit())
            || hex.len() != 32
            || !hex.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return None;
        }
        let port = port.parse().ok()?;
        let device = Uuid::parse_str(hex).ok()?;
        (device.get_version_num() == 7).then_some(Self { device, port })
    }
}

fn authority(req: &Request, port: u16) -> Result<Authority, StatusCode> {
    let mut hosts = req.headers().get_all(header::HOST).iter();
    let header = hosts
        .next()
        .map(|h| Authority::try_from(h.as_bytes()))
        .transpose()
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let uri = req.uri().authority();
    if hosts.next().is_some()
        || matches!((&header, uri), (Some(a), Some(b)) if !a.host().eq_ignore_ascii_case(b.host()))
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    for auth in header.iter().chain(uri) {
        if auth.as_str().contains('@') || auth.host().is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        if auth.as_str().len() != auth.host().len() && auth.port_u16() != Some(port) {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    // Preserve an explicit listener port for the device's external origin.
    uri.filter(|auth| auth.port().is_some())
        .cloned()
        .or(header)
        .or_else(|| uri.cloned())
        .ok_or(StatusCode::BAD_REQUEST)
}

pub(super) fn router(
    edge: Edge,
    control: Router,
    hostname: &str,
    port: u16,
    base: Option<&str>,
) -> Router {
    Router::new().fallback(dispatch).with_state(Hosts {
        edge,
        control,
        control_hostname: hostname.into(),
        port,
        preview_suffix: base.map(|base| format!(".{base}")),
    })
}

#[derive(Clone)]
struct Hosts {
    edge: Edge,
    control: Router,
    control_hostname: String,
    port: u16,
    preview_suffix: Option<String>,
}

async fn dispatch(State(hosts): State<Hosts>, req: Request) -> Response {
    let auth = match authority(&req, hosts.port) {
        Ok(auth) => auth,
        Err(status) => return status.into_response(),
    };
    let host = auth.host().to_ascii_lowercase();
    if host.eq_ignore_ascii_case(&hosts.control_hostname) {
        return hosts.control.oneshot(req).await.unwrap();
    }
    let Some(label) = hosts
        .preview_suffix
        .as_ref()
        .and_then(|s| host.strip_suffix(s))
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if label == "probe"
        && req.method() == Method::GET
        && req.uri().path() == "/__pontia_preview_health"
    {
        return "ok".into_response();
    }
    let Some(target) = Target::parse(label) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mut response = if req.uri().path() == "/__pontia_auth" {
        auth::exchange(&hosts.edge, target, req).await
    } else {
        relay(&hosts.edge, target, auth, req).await
    };
    restrict_embedding(response.headers_mut(), &hosts.edge.origins.dashboard);
    response
}

fn restrict_embedding(headers: &mut axum::http::HeaderMap, dashboard: &str) {
    // Browsers ignore X-Frame-Options when an enforced frame-ancestors policy exists.
    let denies_dashboard = headers
        .get_all(header::X_FRAME_OPTIONS)
        .iter()
        .any(|value| {
            value.as_bytes().split(|&byte| byte == b',').any(|option| {
                let option = option.trim_ascii();
                option.eq_ignore_ascii_case(b"deny") || option.eq_ignore_ascii_case(b"sameorigin")
            })
        });
    let ancestor = if denies_dashboard {
        "'none'"
    } else {
        dashboard
    };
    headers.append(
        header::CONTENT_SECURITY_POLICY,
        format!("frame-ancestors {ancestor}").parse().unwrap(),
    );
}

fn error(status: StatusCode, navigation: bool, message: &str) -> Response {
    let mut response = if navigation {
        (status, Html(format!("<!doctype html><meta charset=utf-8><title>Pontia preview</title><h1>Preview unavailable</h1><p>{message}</p><a href=\"https://app.pontia.dev\">Return to Dashboard</a>"))).into_response()
    } else {
        status.into_response()
    };
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

async fn relay(edge: &Edge, target: Target, auth: Authority, mut req: Request) -> Response {
    let headers = req.headers();
    let navigation = headers
        .get("sec-fetch-mode")
        .is_some_and(|v| v == "navigate")
        || headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("text/html"));
    if auth::strip_cookie(req.headers_mut())
        .and_then(|v| {
            PreviewTokenVerifier(&edge.access_tokens).verify(&v, target.device, target.port)
        })
        .is_none()
    {
        return error(
            StatusCode::UNAUTHORIZED,
            navigation,
            "Reopen this preview from the Dashboard to renew your session.",
        );
    }
    let Some(connection) = edge.online.connection(target.device) else {
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            navigation,
            "The device is offline.",
        );
    };
    let (mut parts, body) = req.into_parts();
    let is_ws = parts.method == Method::GET
        && parts
            .headers
            .get(header::UPGRADE)
            .is_some_and(|v| v.as_bytes().eq_ignore_ascii_case(b"websocket"))
        && parts
            .headers
            .get_all(header::CONNECTION)
            .iter()
            .flat_map(|value| value.as_bytes().split(|&b| b == b','))
            .any(|token| token.trim_ascii().eq_ignore_ascii_case(b"upgrade"));
    let (kind, route) = if is_ws {
        ("ws", TunnelRoute::PreviewWs { port: target.port })
    } else {
        ("http", TunnelRoute::PreviewHttp { port: target.port })
    };
    let path = parts.uri.path_and_query().map_or("/", |p| p.as_str());
    let Ok(uri) = format!(
        "https://pontia-device/preview/v1/{kind}/{}{path}",
        target.port
    )
    .parse::<Uri>() else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let mut forwarded = preview_headers(&parts.headers, route, HeaderDirection::Request);
    forwarded.remove(header::HOST);
    forwarded.insert("x-pontia-preview-host", auth.as_str().parse().unwrap());
    let result = if is_ws {
        let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
            Ok(upgrade) => upgrade,
            Err(e) => return e.into_response(),
        };
        ws::open(connection, uri, forwarded, upgrade).await
    } else {
        parts.uri = uri;
        parts.headers = forwarded;
        connection
            .request_preview_http(Request::from_parts(parts, body))
            .await
    };
    let mut response = match result {
        Ok(response) => response,
        Err(err) => {
            let status = match err {
                TunnelError::Unavailable | TunnelError::Overloaded => {
                    StatusCode::SERVICE_UNAVAILABLE
                }
                TunnelError::Failure => StatusCode::BAD_GATEWAY,
                TunnelError::Timeout => StatusCode::GATEWAY_TIMEOUT,
                TunnelError::InvalidRequest => StatusCode::BAD_REQUEST,
            };
            return error(
                status,
                navigation,
                "The device could not serve this preview. Try again from the Dashboard.",
            );
        }
    };
    if is_ws && response.status() == StatusCode::SWITCHING_PROTOCOLS {
        return response;
    }
    let refused = response
        .headers_mut()
        .remove("x-pontia-preview-error")
        .is_some_and(|v| v == "connection_refused");
    if refused && navigation {
        return error(
            StatusCode::BAD_GATEWAY,
            true,
            &format!("No service is listening on localhost:{}.", target.port),
        );
    }
    *response.headers_mut() = preview_headers(response.headers(), route, HeaderDirection::Response);
    auth::sanitize_cookies(response.headers_mut());
    if navigation && response.status() == StatusCode::BAD_REQUEST {
        return legacy_error(response).await;
    }
    response
}

// Older daemons return this small JSON body for every unknown tunnel route.
// Inspect just its prefix so an application's own 400 response stays streaming.
async fn legacy_error(response: Response) -> Response {
    const LEGACY: &[u8] =
        br#"{"error":{"code":"invalid_tunnel_request","message":"invalid tunneled HTTP request"}}"#;
    let (parts, body) = response.into_parts();
    let mut input = body.into_data_stream();
    let mut seen = Vec::new();
    while let Some(chunk) = input.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                return Response::from_parts(
                    parts,
                    Body::from_stream(stream::iter([Ok(bytes::Bytes::from(seen)), Err(e)])),
                );
            }
        };
        if !LEGACY[seen.len()..].starts_with(&chunk) {
            let prefix = stream::iter([Ok(seen.into()), Ok(chunk)]);
            return Response::from_parts(parts, Body::from_stream(prefix.chain(input)));
        }
        seen.extend_from_slice(&chunk);
    }
    if seen == LEGACY {
        error(
            StatusCode::BAD_REQUEST,
            true,
            "Upgrade pontiad on this device and reopen the preview.",
        )
    } else {
        Response::from_parts(parts, Body::from(seen))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_requires_dashboard_and_preserves_application_restrictions() {
        let dashboard = "https://app.pontia.dev";
        for (options, ancestor) in [
            (None, dashboard),
            (Some("DENY"), "'none'"),
            (Some("SameOrigin"), "'none'"),
        ] {
            let mut headers = axum::http::HeaderMap::new();
            headers.insert(
                header::CONTENT_SECURITY_POLICY,
                "frame-ancestors 'self'; script-src 'self'".parse().unwrap(),
            );
            if let Some(options) = options {
                headers.insert(header::X_FRAME_OPTIONS, options.parse().unwrap());
            }
            restrict_embedding(&mut headers, dashboard);
            let policies: Vec<_> = headers
                .get_all(header::CONTENT_SECURITY_POLICY)
                .iter()
                .map(|v| v.to_str().unwrap())
                .collect();
            assert_eq!(
                policies,
                [
                    "frame-ancestors 'self'; script-src 'self'",
                    &format!("frame-ancestors {ancestor}")
                ]
            );
        }
    }

    #[test]
    fn authority_requires_a_single_matching_host_and_port() {
        for (uri, hosts, expected) in [
            ("/", vec![], Err(StatusCode::BAD_REQUEST)),
            ("/", vec!["a", "a"], Err(StatusCode::BAD_REQUEST)),
            ("https://b/", vec!["a"], Err(StatusCode::BAD_REQUEST)),
            ("/", vec!["user@a"], Err(StatusCode::BAD_REQUEST)),
            ("/", vec!["a:444"], Err(StatusCode::NOT_FOUND)),
            ("/", vec!["a:99999"], Err(StatusCode::NOT_FOUND)),
            ("https://a:443/", vec![], Ok("a:443")),
            ("https://a/", vec!["a"], Ok("a")),
            ("https://a:443/", vec!["A"], Ok("a:443")),
            ("https://a/", vec!["a:444"], Err(StatusCode::NOT_FOUND)),
        ] {
            let mut req = Request::builder().uri(uri);
            for value in hosts {
                req = req.header(header::HOST, value);
            }
            assert_eq!(
                authority(&req.body(Body::empty()).unwrap(), 443)
                    .as_ref()
                    .map(Authority::as_str)
                    .map_err(|s| *s),
                expected
            );
        }
    }
}

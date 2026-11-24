use axum::http::{HeaderMap, HeaderValue, header, uri::Authority};
use pontia_tunnel::protocol::{HeaderDirection, TunnelRoute, preview_headers};

pub(super) fn preview_host(headers: &HeaderMap) -> Option<Authority> {
    let mut values = headers.get_all("x-pontia-preview-host").iter();
    let host = values.next()?.to_str().ok()?;
    if values.next().is_some() || host.contains('@') {
        return None;
    }
    host.parse().ok()
}

pub(super) fn request_headers(
    headers: &HeaderMap,
    route: TunnelRoute,
    port: u16,
    host: &Authority,
) -> HeaderMap {
    let mut headers = preview_headers(headers, route, HeaderDirection::Request);
    let external = format!("https://{host}");
    let local = format!("localhost:{port}");
    for name in [header::ORIGIN, header::REFERER] {
        if let Some(value) = headers.get_mut(&name)
            && let Ok(value_str) = value.to_str()
            && let Some(suffix) = value_str.strip_prefix(&external)
            && (suffix.is_empty() || (name == header::REFERER && suffix.starts_with('/')))
        {
            *value = format!("http://{local}{suffix}").parse().unwrap();
        }
    }
    headers.insert(header::HOST, local.parse().unwrap());
    headers.insert("x-forwarded-host", host.as_str().parse().unwrap());
    headers.insert("x-forwarded-proto", HeaderValue::from_static("https"));
    headers
}

pub(super) fn rewrite_location(headers: &mut HeaderMap, port: u16, host: &Authority) {
    let Some(value) = headers.get_mut(header::LOCATION) else {
        return;
    };
    let Ok(location) = value.to_str() else { return };
    let Some((scheme, rest)) = location.split_once("://") else {
        return;
    };
    if !scheme.eq_ignore_ascii_case("http") {
        return;
    }
    let (authority, suffix) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    let Ok(addr) = authority.parse::<Authority>() else {
        return;
    };
    if !authority.contains('@')
        && ["localhost", "127.0.0.1", "[::1]", "0.0.0.0"]
            .iter()
            .any(|h| h.eq_ignore_ascii_case(addr.host()))
        && (authority.eq_ignore_ascii_case(addr.host()) && port == 80
            || addr.port_u16() == Some(port))
    {
        *value = format!("https://{host}{suffix}").parse().unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_third_party_origin_and_similar_referer() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://other.test"),
        );
        headers.insert(
            header::REFERER,
            HeaderValue::from_static("https://preview.test.evil/path"),
        );
        let rewritten = request_headers(
            &headers,
            TunnelRoute::PreviewHttp { port: 80 },
            80,
            &"preview.test".parse().unwrap(),
        );
        assert_eq!(rewritten[header::ORIGIN], headers[header::ORIGIN]);
        assert_eq!(rewritten[header::REFERER], headers[header::REFERER]);
    }

    #[test]
    fn only_rewrites_absolute_http_loopback_redirects_on_target_port() {
        for (location, expected) in [
            ("http://LOCALHOST?q=1", "https://preview.test?q=1"),
            ("http://[::1]:80/#f", "https://preview.test/#f"),
            ("http://0.0.0.0/a", "https://preview.test/a"),
            ("http://127.0.0.1:80/%2F", "https://preview.test/%2F"),
            ("/relative", "/relative"),
            ("http://localhost:81/a", "http://localhost:81/a"),
            ("http://user@localhost/a", "http://user@localhost/a"),
            ("https://localhost/a", "https://localhost/a"),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::LOCATION, location.parse().unwrap());
            rewrite_location(&mut headers, 80, &"preview.test".parse().unwrap());
            assert_eq!(headers[header::LOCATION], expected);
        }
    }
}

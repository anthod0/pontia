use super::{Activity, Target, error};
use crate::{Edge, browser_http::exact_origin};
use axum::{
    Form,
    extract::{DefaultBodyLimit, FromRequest, Request},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

const COOKIE_PREFIX: &str = "__Host-pontia_preview";

pub(super) fn legacy_cookie() -> &'static str {
    COOKIE_PREFIX
}

pub(super) fn port_cookie(port: u16) -> String {
    format!("{COOKIE_PREFIX}_{port}")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exchange {
    token: String,
    redirect: Option<String>,
}

pub(super) async fn exchange(
    edge: &Edge,
    target: Target,
    cookie_name: &str,
    activity: Option<Activity>,
    mut req: Request,
) -> Response {
    let failure = |s| error(s, true, "Reopen the preview from the Dashboard.");
    if req.method() != Method::POST {
        return failure(StatusCode::METHOD_NOT_ALLOWED);
    }
    if !exact_origin(req.headers(), &edge.origins.dashboard) {
        return failure(StatusCode::FORBIDDEN);
    }
    DefaultBodyLimit::max(16 * 1024).apply(&mut req);
    let form = match Form::<Exchange>::from_request(req, &()).await {
        Ok(Form(form)) => form,
        Err(err) if err.status() == StatusCode::PAYLOAD_TOO_LARGE => return failure(err.status()),
        Err(_) => return failure(StatusCode::BAD_REQUEST),
    };
    let Some(age) = edge
        .access_tokens
        .preview
        .verify(&form.token, target.device, target.port)
    else {
        return failure(StatusCode::UNAUTHORIZED);
    };
    if let Some(activity) = activity {
        activity.touch();
    }
    let redirect = form.redirect.as_deref().unwrap_or("/");
    let path = redirect.split(['?', '#']).next().unwrap();
    if redirect.len() > 4096
        || !redirect.starts_with('/')
        || redirect.starts_with("//")
        || redirect.starts_with("/\\")
        || redirect.bytes().any(|b| b.is_ascii_control())
        || path == "/__pontia_auth"
        || path.starts_with("/__pontia_auth/")
    {
        return failure(StatusCode::BAD_REQUEST);
    }
    let Ok(location) = HeaderValue::from_str(redirect) else {
        return failure(StatusCode::BAD_REQUEST);
    };
    let cookie = format!(
        "{cookie_name}={}; Path=/; HttpOnly; Secure; SameSite=None; Partitioned; Max-Age={age}",
        form.token,
    );
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location),
            (header::SET_COOKIE, cookie.parse().unwrap()),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            (
                header::REFERRER_POLICY,
                HeaderValue::from_static("no-referrer"),
            ),
            (
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            ),
        ],
    )
        .into_response()
}

pub(super) fn strip_cookie(headers: &mut HeaderMap, expected_name: &str) -> Option<String> {
    let mut token = None;
    let mut rest = Vec::new();
    for line in headers.get_all(header::COOKIE) {
        for pair in line
            .to_str()
            .ok()?
            .split(';')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            match pair.split_once('=') {
                Some((name, value))
                    if name.trim() == COOKIE_PREFIX
                        || name.trim().starts_with(&format!("{COOKIE_PREFIX}_")) =>
                {
                    if name.trim() == expected_name && token.replace(value.to_owned()).is_some() {
                        return None;
                    }
                }
                _ => rest.push(pair),
            }
        }
    }
    let remaining = rest.join("; ");
    headers.remove(header::COOKIE);
    if !remaining.is_empty() {
        headers.insert(header::COOKIE, remaining.parse().ok()?);
    }
    token
}

pub(super) fn sanitize_cookies(headers: &mut HeaderMap) {
    let cookies: Vec<_> = headers
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|value| {
            let mut parts = value.to_str().ok()?.split(';');
            let pair = parts.next()?.trim();
            let (name, _) = pair.split_once('=')?;
            if name.trim().is_empty()
                || name
                    .trim()
                    .to_ascii_lowercase()
                    .starts_with(&COOKIE_PREFIX.to_ascii_lowercase())
            {
                return None;
            }
            let mut cookie = pair.to_owned();
            for attr in parts.map(str::trim) {
                if attr
                    .split('=')
                    .next()
                    .unwrap()
                    .trim()
                    .eq_ignore_ascii_case("domain")
                {
                    continue;
                }
                cookie.push_str("; ");
                cookie.push_str(attr);
            }
            HeaderValue::from_str(&cookie).ok()
        })
        .collect();
    headers.remove(header::SET_COOKIE);
    for cookie in cookies {
        headers.append(header::SET_COOKIE, cookie);
    }
}

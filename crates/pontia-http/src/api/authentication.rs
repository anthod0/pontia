use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
};

use crate::HttpState;

use super::response::ApiError;

#[derive(Clone, Copy)]
pub(crate) struct TrustedTunnelRequest;

#[derive(Clone, Copy)]
enum ExternalApiPrincipal {
    LocalToken,
    DeviceTunnel,
}

pub(crate) async fn authenticate(
    State(state): State<HttpState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let principal = if request.extensions().get::<TrustedTunnelRequest>().is_some() {
        ExternalApiPrincipal::DeviceTunnel
    } else {
        authenticate_local_token(state.app().external_api_token(), request.headers())?
    };

    request.extensions_mut().insert(principal);
    Ok(next.run(request).await)
}

fn authenticate_local_token(
    expected: Option<&str>,
    headers: &axum::http::HeaderMap,
) -> Result<ExternalApiPrincipal, ApiError> {
    let Some(expected) = expected else {
        return Err(ApiError::authentication_failed(
            "external API token is not configured",
        ));
    };

    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);

    if authorized {
        Ok(ExternalApiPrincipal::LocalToken)
    } else {
        Err(ApiError::authentication_failed(
            "missing or invalid bearer token",
        ))
    }
}

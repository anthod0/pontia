use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
};

use pontia_application::AppState;

use super::response::ApiError;

#[derive(Clone, Copy)]
enum ExternalApiPrincipal {
    LocalToken,
    E2e,
}

pub(crate) async fn authenticate(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if request
        .headers()
        .contains_key(pontia_bot::identity::TOOL_TOKEN_HEADER)
    {
        return Err(ApiError::authentication_failed(
            "Bot tool credentials cannot access this route",
        ));
    }
    let principal = if request
        .extensions()
        .get::<crate::e2e::AuthenticatedE2eRequest>()
        .is_some()
    {
        ExternalApiPrincipal::E2e
    } else {
        authenticate_local_token(state.external_api_token(), request.headers())?
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

use axum::http::{HeaderMap, header};

use pontia_application::AppState;

use super::response::ExternalApiError;

pub(super) fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<(), ExternalApiError> {
    let Some(expected) = state.external_api_token() else {
        return Err(ExternalApiError::authentication_failed(
            "external API token is not configured",
        ));
    };

    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);

    if authorized {
        Ok(())
    } else {
        Err(ExternalApiError::authentication_failed(
            "missing or invalid bearer token",
        ))
    }
}

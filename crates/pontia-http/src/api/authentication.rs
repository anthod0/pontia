use axum::http::{HeaderMap, header};

use pontia_application::AppState;

use super::response::ApiError;

pub(super) fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = state.external_api_token() else {
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
        Ok(())
    } else {
        Err(ApiError::authentication_failed(
            "missing or invalid bearer token",
        ))
    }
}

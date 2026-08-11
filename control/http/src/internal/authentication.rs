use axum::http::{HeaderMap, header};
use pontia_application::AppState;

use super::response::ApiError;

pub(crate) fn authenticate_internal_token(
    state: &AppState,
    headers: &HeaderMap,
    not_configured_message: &'static str,
) -> Result<(), ApiError> {
    let expected = state
        .external_api_token()
        .ok_or_else(|| ApiError::authentication_failed(not_configured_message))?;
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

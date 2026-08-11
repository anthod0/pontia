use std::future::Future;

use axum::http::HeaderMap;
use serde_json::Value;

use pontia_application::{AppState, IdempotencyOutcome};
use pontia_core::error::Error;

use super::response::ExternalApiError;

fn idempotency_key(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok())
}

pub(super) async fn idempotent<F, Fut>(
    state: &AppState,
    headers: &HeaderMap,
    operation: impl Into<String>,
    action: F,
) -> Result<IdempotencyOutcome, ExternalApiError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Value, Error>>,
{
    state
        .idempotency()
        .run(operation, idempotency_key(headers), action)
        .await
        .map_err(Into::into)
}

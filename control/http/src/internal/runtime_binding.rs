use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
};
use pontia_application::{AppState, RuntimeBindingUpsertRequest, RuntimeBindingUpsertService};
use serde_json::Value;

use super::response::ApiError;

pub async fn upsert_runtime_binding(
    State(state): State<AppState>,
    request: Result<Json<RuntimeBindingUpsertRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let response = RuntimeBindingUpsertService::new(state.db(), state.pontia_home().to_path_buf())
        .upsert(request)
        .await?;
    Ok(Json(response))
}

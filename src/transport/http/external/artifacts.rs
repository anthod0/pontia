use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use crate::application::{
    AppState, ArtifactContentService, ArtifactDiscoveryService, ExternalQueryService,
};

use super::common::{ApiResponse, ExternalApiError, authenticate, ensure_session_exists, ok};

pub async fn list_artifacts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    let artifacts = service.list_artifacts(&session_id).await?;
    Ok(ok(json!({ "artifacts": artifacts })))
}

pub async fn discover_artifacts(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ArtifactDiscoveryService::new(state.db);
    let outcome = service.discover(&session_id).await?;
    Ok(ok(json!({ "artifacts": outcome.artifacts })))
}

pub async fn get_artifact(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(artifact_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    let artifact = service
        .get_artifact(&artifact_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("artifact {artifact_id} not found")))?;
    Ok(ok(json!({ "artifact": artifact })))
}

pub async fn get_artifact_content(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(artifact_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ArtifactContentService::new(state.db);
    let content = service.read_content(&artifact_id).await?;
    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/octet-stream")],
        content.bytes,
    )
        .into_response())
}

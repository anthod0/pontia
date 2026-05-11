use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use crate::application::{AgentProfileService, AppState, UpsertExecutionProfileRequest};

use super::common::{ApiResponse, ExternalApiError, authenticate, idempotency_key, ok};

pub async fn list_agent_profiles(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db);
    let profiles = service.list_latest().await?;
    Ok(ok(json!({ "agent_profiles": profiles })))
}

pub async fn get_agent_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db);
    let profile = service.get_latest(&profile_id).await?.ok_or_else(|| {
        ExternalApiError::not_found(format!("agent profile {profile_id} not found"))
    })?;
    Ok(ok(json!({ "agent_profile": profile })))
}

pub async fn create_agent_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpsertExecutionProfileRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db);
    let outcome = service
        .create_profile(request, idempotency_key(&headers))
        .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn create_agent_profile_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
    Json(request): Json<UpsertExecutionProfileRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db);
    let outcome = service
        .create_profile_version(&profile_id, request, idempotency_key(&headers))
        .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};

use pontia_agent_control::{AgentProfileService, UpsertAgentProfileRequest};
use pontia_application::AppState;

use super::{
    idempotency::idempotent,
    response::{ApiError, ApiResponse, ok},
};

pub async fn list_agent_profiles(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let profiles = AgentProfileService::new(state.db())
        .with_clients(state.control().clients())
        .list()
        .await?;
    Ok(ok(json!({ "agent_profiles": profiles })))
}

pub async fn get_agent_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = AgentProfileService::new(state.db()).with_clients(state.control().clients());
    let profile = service
        .get(&profile_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("agent profile {profile_id} not found")))?;
    Ok(ok(json!({ "agent_profile": profile })))
}

pub async fn create_agent_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<UpsertAgentProfileRequest>,
) -> Result<Response, ApiError> {
    let service = AgentProfileService::new(state.db()).with_clients(state.control().clients());
    let outcome = idempotent(&state, &headers, "create_agent_profile", || async move {
        Ok(json!({"agent_profile": service.create(request).await?}))
    })
    .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn delete_agent_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
) -> Result<Response, ApiError> {
    let service = AgentProfileService::new(state.db()).with_clients(state.control().clients());
    let operation = format!("delete_agent_profile:{profile_id}");
    let outcome = idempotent(&state, &headers, operation, || async move {
        service.delete(&profile_id).await?;
        Ok(json!({"profile_id": profile_id}))
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn update_agent_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
    Json(request): Json<UpsertAgentProfileRequest>,
) -> Result<Response, ApiError> {
    let service = AgentProfileService::new(state.db()).with_clients(state.control().clients());
    let operation = format!("update_agent_profile:{profile_id}");
    let outcome = idempotent(&state, &headers, operation, || async move {
        Ok(json!({"agent_profile": service.update(&profile_id, request).await?}))
    })
    .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

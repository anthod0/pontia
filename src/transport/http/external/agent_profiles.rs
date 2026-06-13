use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::application::{AgentProfileService, AppState, UpsertExecutionProfileRequest};

use super::common::{ApiResponse, ExternalApiError, authenticate, idempotency_key, ok};

#[derive(Debug, Deserialize)]
pub struct AgentProfilesQuery {
    #[serde(default)]
    include_archived: bool,
}

#[derive(Debug, Deserialize)]
pub struct AgentProfileVersionsQuery {
    #[serde(default)]
    include_archived: bool,
}

pub async fn list_agent_profiles(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<AgentProfilesQuery>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db());
    let profiles = if query.include_archived {
        service.list_latest_including_archived().await?
    } else {
        service.list_latest().await?
    };
    Ok(ok(json!({ "agent_profiles": profiles })))
}

pub async fn get_agent_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db());
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
    let service = AgentProfileService::new(state.db());
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

pub async fn delete_agent_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db());
    let outcome = service
        .archive_profile(&profile_id, idempotency_key(&headers))
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn list_agent_profile_versions(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
    Query(query): Query<AgentProfileVersionsQuery>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db());
    let versions = service
        .list_versions(&profile_id, query.include_archived)
        .await?;
    if versions.is_empty() {
        return Err(ExternalApiError::not_found(format!(
            "agent profile {profile_id} not found"
        )));
    }
    Ok(ok(json!({ "agent_profile_versions": versions })))
}

pub async fn create_agent_profile_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(profile_id): Path<String>,
    Json(request): Json<UpsertExecutionProfileRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db());
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

pub async fn get_agent_profile_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((profile_id, version)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db());
    let profile = service
        .get_version(&profile_id, &version)
        .await?
        .ok_or_else(|| {
            ExternalApiError::not_found(format!("agent profile {profile_id}@{version} not found"))
        })?;
    Ok(ok(json!({ "agent_profile": profile })))
}

pub async fn update_agent_profile_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((profile_id, version)): Path<(String, String)>,
    Json(request): Json<UpsertExecutionProfileRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db());
    let outcome = service
        .update_version(&profile_id, &version, request, idempotency_key(&headers))
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn delete_agent_profile_version(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((profile_id, version)): Path<(String, String)>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = AgentProfileService::new(state.db());
    let outcome = service
        .archive_version(&profile_id, &version, idempotency_key(&headers))
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

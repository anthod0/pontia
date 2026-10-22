use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
};
use pontia_application::{AppState, agent_clients::AgentClientService};
use serde_json::{Value, json};

use super::response::{ApiError, ApiResponse, ok};

pub async fn list_agent_clients(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = AgentClientService::new(state.db(), state.clients());
    Ok(ok(json!({"agent_clients": service.list().await?})))
}

pub async fn get_agent_client(
    State(state): State<AppState>,
    Path((client, key)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = AgentClientService::new(state.db(), state.clients());
    Ok(ok(
        json!({"agent_client": service.get(&client, &key).await?}),
    ))
}

pub async fn patch_agent_client(
    State(state): State<AppState>,
    Path((client, key)): Path<(String, String)>,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(value) = body.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    let object = value
        .as_object()
        .ok_or_else(|| ApiError::invalid_request("Expected a settings object"))?;
    if object
        .keys()
        .any(|key| key != "enabled" && key != "config_overrides")
    {
        return Err(ApiError::invalid_request("Unknown client setting"));
    }
    let enabled = object
        .get("enabled")
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| ApiError::invalid_request("enabled must be a boolean"))
        })
        .transpose()?;
    let overrides = object.get("config_overrides").cloned();
    if !state
        .clients()
        .get(&client)
        .is_some_and(|entry| entry.configuration.is_some())
    {
        return Err(ApiError::invalid_request("Unknown client integration"));
    }
    let service = AgentClientService::new(state.db(), state.clients());
    service.save(&client, &key, enabled, overrides).await?;
    Ok(ok(
        json!({"agent_client": service.get(&client, &key).await?}),
    ))
}

pub async fn delete_agent_client(
    State(state): State<AppState>,
    Path((client, key)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = AgentClientService::new(state.db(), state.clients());
    service.delete(&client, &key).await?;
    Ok(ok(json!({"deleted": true})))
}

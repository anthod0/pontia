use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use pontia_agent_control::{agent_clients::AgentClientService, client_contract::DetectionContext};
use pontia_application::AppState;
use serde_json::{Value, json};

use super::response::{ApiError, ApiResponse, ok};

#[derive(serde::Deserialize)]
pub struct ModelsQuery {
    workspace_id: Option<String>,
    #[serde(default)]
    prefer_cache: bool,
}

pub async fn list_agent_client_models(
    State(state): State<AppState>,
    Path((client, key)): Path<(String, String)>,
    Query(query): Query<ModelsQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let directory = match query.workspace_id {
        Some(id) => state
            .control()
            .queries()
            .get_workspace(&id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("workspace {id} not found")))?
            .canonical_path
            .into(),
        None => state.pontia_home().to_path_buf(),
    };
    let service = AgentClientService::new(state.db(), state.control().clients());
    let models = service
        .model_catalog(&client, &key, &directory, query.prefer_cache)
        .await?;
    Ok(ok(json!(models)))
}

pub async fn list_agent_clients(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = AgentClientService::new(state.db(), state.control().clients());
    let vars = std::env::vars().collect();
    let context = DetectionContext {
        vars: &vars,
        pontia_home: state.pontia_home(),
    };
    let entries = service.list(&context).await?;
    service.probe_enabled_in_background(state.pontia_home().to_path_buf(), false);
    Ok(ok(json!({"agent_clients": entries})))
}

pub async fn get_agent_client(
    State(state): State<AppState>,
    Path((client, key)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = AgentClientService::new(state.db(), state.control().clients());
    let vars = std::env::vars().collect();
    let context = DetectionContext {
        vars: &vars,
        pontia_home: state.pontia_home(),
    };
    Ok(ok(
        json!({"agent_client": service.get(&client, &key, &context).await?}),
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
    if object.keys().any(|key| key != "enabled") {
        return Err(ApiError::invalid_request("Unknown client setting"));
    }
    let enabled = object
        .get("enabled")
        .ok_or_else(|| ApiError::invalid_request("enabled is required"))?
        .as_bool()
        .ok_or_else(|| ApiError::invalid_request("enabled must be a boolean"))?;
    if !state
        .control()
        .clients()
        .get(&client)
        .is_some_and(|entry| entry.configuration.is_some())
    {
        return Err(ApiError::invalid_request("Unknown client integration"));
    }
    let service = AgentClientService::new(state.db(), state.control().clients());
    service.save(&client, &key, enabled).await?;
    let vars = std::env::vars().collect();
    let context = DetectionContext {
        vars: &vars,
        pontia_home: state.pontia_home(),
    };
    if enabled && let Err(error) = service.probe_version(&client, &key, &context, true).await {
        tracing::warn!(%error, %client, %key, "Cannot check enabled client version");
    }
    Ok(ok(
        json!({"agent_client": service.get(&client, &key, &context).await?}),
    ))
}

pub async fn delete_agent_client(
    State(state): State<AppState>,
    Path((client, key)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = AgentClientService::new(state.db(), state.control().clients());
    service.delete(&client, &key).await?;
    Ok(ok(json!({"deleted": true})))
}

pub async fn setup_agent_client(
    State(state): State<AppState>,
    Path((client, key)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = AgentClientService::new(state.db(), state.control().clients());
    let operation = service
        .setup(state.pontia_home().to_path_buf(), &client, &key)
        .await?;
    Ok(ok(json!({"operation": operation})))
}

pub async fn agent_client_setup_status(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let operation = AgentClientService::setup_status(state.pontia_home().to_path_buf()).await?;
    Ok(ok(json!({"operation": operation})))
}

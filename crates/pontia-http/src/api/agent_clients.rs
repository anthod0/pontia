use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode, header},
    response::sse::{Event, KeepAlive, Sse},
};
use pontia_agent_control::{
    agent_clients::{AgentClientService, ClientInstallStartError},
    client_contract::DetectionContext,
};
use pontia_application::AppState;
use serde_json::{Value, json};
use std::{convert::Infallible, time::Duration};
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use uuid::Uuid;

use super::response::{ApiError, ApiResponse, ok};

#[derive(serde::Deserialize)]
pub struct ModelsQuery {
    workspace_id: Option<String>,
    #[serde(default)]
    prefer_cache: bool,
}

#[derive(Clone, Copy, Default, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum InstallAction {
    #[default]
    Install,
    Upgrade,
    Reinstall,
}

#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallRequest {
    #[serde(default)]
    action: InstallAction,
    method_id: Option<String>,
    #[serde(default = "default_true")]
    auto_setup: bool,
    #[serde(default = "default_true")]
    auto_enable: bool,
}

const fn default_true() -> bool {
    true
}

#[derive(Default, serde::Deserialize)]
pub struct OperationQuery {
    offset: Option<u64>,
    tail_bytes: Option<u64>,
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
    tokio::spawn(service.probe_enabled(state.pontia_home().to_path_buf(), false));
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

pub async fn install_agent_client(
    State(state): State<AppState>,
    Path((client, key)): Path<(String, String)>,
    body: Result<Option<Json<InstallRequest>>, JsonRejection>,
) -> Result<(StatusCode, Json<ApiResponse<Value>>), ApiError> {
    let request = body
        .map_err(|error| ApiError::invalid_request(error.body_text()))?
        .map(|Json(request)| request)
        .unwrap_or_default();
    if !matches!(request.action, InstallAction::Install) {
        return Err(ApiError::custom(
            StatusCode::UNPROCESSABLE_ENTITY,
            "action_unavailable",
            "This client installer does not support upgrade or reinstall",
        ));
    }
    let service = AgentClientService::new(state.db(), state.control().clients());
    let operation = service
        .start_install(
            state.pontia_home().to_path_buf(),
            &client,
            &key,
            request.method_id.as_deref(),
            request.auto_setup,
            request.auto_enable,
        )
        .await
        .map_err(install_error)?;
    Ok((StatusCode::ACCEPTED, ok(json!({"operation": operation}))))
}

fn install_error(error: ClientInstallStartError) -> ApiError {
    match error {
        ClientInstallStartError::Control(error) => error.into(),
        ClientInstallStartError::Unavailable(message) => ApiError::custom(
            StatusCode::UNPROCESSABLE_ENTITY,
            "capability_unavailable",
            message,
        ),
        ClientInstallStartError::PrerequisiteMissing(message) => ApiError::custom(
            StatusCode::UNPROCESSABLE_ENTITY,
            "prerequisite_missing",
            message,
        ),
        ClientInstallStartError::InProgress(message) => {
            ApiError::custom(StatusCode::CONFLICT, "operation_in_progress", message)
        }
        ClientInstallStartError::Io(error) => pontia_core::Error::Io(error).into(),
    }
}

pub async fn latest_agent_client_operation(
    State(state): State<AppState>,
    Query(query): Query<OperationQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let operation = AgentClientService::latest_operation(
        state.pontia_home().to_path_buf(),
        query.tail_bytes.unwrap_or(64 * 1024).min(64 * 1024),
    )
    .await?;
    Ok(ok(json!({"operation": operation})))
}

pub async fn get_agent_client_operation(
    State(state): State<AppState>,
    Path(operation_id): Path<String>,
    Query(query): Query<OperationQuery>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let id = parse_operation_id(&operation_id)?;
    let operation =
        AgentClientService::operation(state.pontia_home().to_path_buf(), id, query.offset).await?;
    Ok(ok(json!({"operation": operation})))
}

pub async fn cancel_agent_client_operation(
    State(state): State<AppState>,
    Path(operation_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let id = parse_operation_id(&operation_id)?;
    let operation =
        AgentClientService::cancel_operation(state.pontia_home().to_path_buf(), id).await?;
    Ok(ok(json!({"operation": operation})))
}

pub async fn stream_agent_client_operation(
    State(state): State<AppState>,
    Path(operation_id): Path<String>,
    Query(query): Query<OperationQuery>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let id = parse_operation_id(&operation_id)?;
    let header_offset = headers
        .get(header::HeaderName::from_static("last-event-id"))
        .and_then(|value| value.to_str().ok())
        .map(str::parse::<u64>)
        .transpose()
        .map_err(|_| ApiError::invalid_request("Last-Event-ID must be a byte offset"))?;
    let offset = query.offset.or(header_offset).unwrap_or(0);
    AgentClientService::operation(state.pontia_home().to_path_buf(), id, Some(offset)).await?;
    let (sender, receiver) = mpsc::channel(32);
    let home = state.pontia_home().to_path_buf();
    tokio::spawn(async move {
        let mut offset = offset;
        let mut previous_state = None;
        loop {
            let view = match AgentClientService::operation(home.clone(), id, Some(offset)).await {
                Ok(view) => view,
                Err(error) => {
                    tracing::warn!(%error, operation_id = %id, "Cannot read Agent Client operation stream");
                    return;
                }
            };
            let state = json!({"operation": view.operation, "log_size": view.log_size});
            if previous_state.as_ref() != Some(&state) {
                let Ok(event) = Event::default().event("state").json_data(&state) else {
                    return;
                };
                if sender.send(Ok(event)).await.is_err() {
                    return;
                }
                previous_state = Some(state);
            }
            if !view.output.is_empty() {
                let next_offset = view.next_offset;
                let payload = json!({
                    "offset": view.output_offset,
                    "next_offset": next_offset,
                    "chunk": view.output,
                });
                let Ok(event) = Event::default()
                    .event("log")
                    .id(next_offset.to_string())
                    .json_data(payload)
                else {
                    return;
                };
                if sender.send(Ok(event)).await.is_err() {
                    return;
                }
                offset = next_offset;
                continue;
            }
            if view.finished() && offset >= view.log_size {
                let Ok(event) = Event::default().event("done").json_data(json!({
                    "operation": view.operation,
                    "log_size": view.log_size,
                })) else {
                    return;
                };
                let _ = sender.send(Ok(event)).await;
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });
    Ok(Sse::new(ReceiverStream::new(receiver)).keep_alive(KeepAlive::default()))
}

fn parse_operation_id(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::invalid_request("Invalid operation ID"))
}

pub async fn agent_client_setup_status(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let operation = AgentClientService::setup_status(state.pontia_home().to_path_buf()).await?;
    Ok(ok(json!({"operation": operation})))
}

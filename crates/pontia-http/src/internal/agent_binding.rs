use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
};
use pontia_application::{
    AgentBindingService, AppState, CurrentTurnClaimRequest, CurrentTurnClaimService,
};
use pontia_core::error::Error;
use serde::Deserialize;
use serde_json::{Value, json};

use super::response::ApiError;

#[derive(Debug, Deserialize)]
pub struct AgentBindingQuery {
    client_type: String,
    client_session_key: String,
}

pub async fn get_agent_binding(
    State(state): State<AppState>,
    Query(query): Query<AgentBindingQuery>,
) -> Result<Json<Value>, ApiError> {
    let client_type = required_query_param("client_type", &query.client_type)?;
    let client_session_key = required_query_param("client_session_key", &query.client_session_key)?;
    let binding = AgentBindingService::new(state.db())
        .binding_for_client_session(client_type, client_session_key)
        .await?
        .ok_or_else(|| Error::NotFound("agent binding not found".to_string()))?;

    Ok(Json(json!({ "data": { "binding": binding } })))
}

pub async fn get_agent_binding_session_context(
    State(state): State<AppState>,
    Query(query): Query<AgentBindingQuery>,
) -> Result<Json<Value>, ApiError> {
    let client_type = required_query_param("client_type", &query.client_type)?;
    let client_session_key = required_query_param("client_session_key", &query.client_session_key)?;
    let session_context = AgentBindingService::new(state.db())
        .session_context_for_client_session(client_type, client_session_key)
        .await?
        .ok_or_else(|| {
            Error::NotFound("session context for agent binding not found".to_string())
        })?;

    Ok(Json(
        json!({ "data": { "session_context": session_context } }),
    ))
}

pub async fn get_agent_binding_current_turn(
    State(state): State<AppState>,
    Query(query): Query<AgentBindingQuery>,
) -> Result<Json<Value>, ApiError> {
    let client_type = required_query_param("client_type", &query.client_type)?;
    let client_session_key = required_query_param("client_session_key", &query.client_session_key)?;
    let current_turn = AgentBindingService::new(state.db())
        .current_turn_for_client_session(client_type, client_session_key)
        .await?
        .ok_or_else(|| Error::NotFound("active turn for agent binding not found".to_string()))?;

    Ok(Json(json!({ "data": { "current_turn": current_turn } })))
}

pub async fn claim_current_turn(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
    request: Result<Json<CurrentTurnClaimRequest>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let current_turn = CurrentTurnClaimService::new(state.db())
        .claim(&session_id, request)
        .await?;
    Ok(Json(json!({ "data": { "current_turn": current_turn } })))
}

fn required_query_param<'a>(name: &str, value: &'a str) -> Result<&'a str, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::invalid_request(format!(
            "{name} query parameter is required"
        )));
    }
    Ok(value)
}

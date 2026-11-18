use std::convert::Infallible;

use axum::{
    Json,
    extract::{Path, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use pontia_agent_control::{agent_clients::AgentClientService, client_contract::DetectionContext};
use pontia_bot::{
    conversation::{ConversationUpdate, ReplyRequest},
    records::{BotConfiguration, CreateError},
};
use pontia_core::domain::EventType;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::Stream;

use super::response::{ApiError, ApiResponse, ok};
use crate::HttpState;

pub async fn list(State(state): State<HttpState>) -> Result<Json<ApiResponse<Value>>, ApiError> {
    Ok(ok(json!({"bots": state.bots().records().list().await?})))
}

pub async fn clients(State(state): State<HttpState>) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let app = state.app();
    let service = AgentClientService::new(app.db(), app.control().clients());
    let vars = std::env::vars().collect();
    let clients: Vec<_> = service.list(&DetectionContext { vars: &vars, pontia_home: app.pontia_home() })
        .await?.into_iter()
        .filter(|client| client.enabled && state.bots().supports_client(&client.client_type))
        .map(|client| json!({"id": client.id, "client_type": client.client_type, "client_key": client.client_key, "name": client.name}))
        .collect();
    Ok(ok(json!({"clients": clients})))
}

pub async fn create(
    State(state): State<HttpState>,
    request: Result<Json<BotConfiguration>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    validate_client(&state, &request)?;
    let bot = state
        .bots()
        .records()
        .create(request)
        .await
        .map_err(|error| match error {
            CreateError::Rejected(error) => error.into(),
            error => ApiError::custom(
                StatusCode::INTERNAL_SERVER_ERROR,
                "bot_initialization_failed",
                error.to_string(),
            ),
        })?;
    Ok(ok(json!({"bot": bot})))
}

pub async fn update(
    State(state): State<HttpState>,
    Path(bot_id): Path<String>,
    request: Result<Json<BotConfiguration>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    validate_client(&state, &request)?;
    Ok(ok(
        json!({"bot": state.bots().records().update(&bot_id, request).await?}),
    ))
}

fn validate_client(state: &HttpState, request: &BotConfiguration) -> Result<(), ApiError> {
    if !state.bots().supports_client(&request.client_type) {
        return Err(ApiError::invalid_request(
            "This client cannot run a Bot Session",
        ));
    }
    Ok(())
}

pub async fn chat(
    State(state): State<HttpState>,
    Path(bot_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    Ok(ok(json!(state.bots().chat(&bot_id).await?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendInput {
    content: String,
}

pub async fn send(
    State(state): State<HttpState>,
    Path(bot_id): Path<String>,
    request: Result<Json<SendInput>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    Ok(ok(
        json!({"message": state.bots().send(&bot_id, &request.content).await?}),
    ))
}

pub async fn retry(
    State(state): State<HttpState>,
    Path((bot_id, message_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    Ok(ok(
        json!({"message": state.bots().retry(&bot_id, &message_id).await?}),
    ))
}

pub async fn reply(
    State(state): State<HttpState>,
    headers: HeaderMap,
    request: Result<Json<ReplyRequest>, JsonRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Json(request) = request.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    super::bot_context::authorize_tool(&state, &headers, &request.session_id).await?;
    let message = state
        .bots()
        .send_to_user(request)
        .await
        .map_err(|error| match error {
            pontia_core::Error::Conflict {
                code: "bot_session_required",
                message,
            } => ApiError::custom(StatusCode::FORBIDDEN, "forbidden", message),
            error => error.into(),
        })?;
    Ok(ok(json!({"sent": true, "message_id": message.id})))
}

pub async fn stream(
    State(state): State<HttpState>,
    Path(bot_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    state.bots().records().get(&bot_id).await?;
    let mut updates = state.bots().subscribe(&bot_id);
    let mut events = state.app().control().agent_events().subscribe();
    let stream = async_stream::stream! {
        yield Ok(update_event(ConversationUpdate::Changed));
        loop {
            tokio::select! {
                update = updates.recv() => match update {
                    Ok(update) => yield Ok(update_event(update)),
                    Err(RecvError::Lagged(_)) => yield Ok(update_event(ConversationUpdate::Changed)),
                    Err(RecvError::Closed) => break,
                },
                event = events.recv() => match event {
                    Ok(event) if matches!(event.event_type, EventType::TurnFailed | EventType::SessionError | EventType::SessionExited) => {
                        match state.bots().records().get(&bot_id).await {
                            Ok(bot) if bot.current_session_id.as_deref() == Some(&event.session_id) =>
                                yield Ok(update_event(ConversationUpdate::Changed)),
                            Ok(_) => (),
                            Err(error) => {
                                tracing::warn!(%bot_id, %error, "Bot event stream lookup failed");
                                break;
                            }
                        }
                    }
                    Ok(_) => (),
                    Err(RecvError::Lagged(_)) => yield Ok(update_event(ConversationUpdate::Changed)),
                    Err(RecvError::Closed) => break,
                },
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn update_event(update: ConversationUpdate) -> Event {
    Event::default()
        .json_data(update)
        .expect("serializable conversation update")
}

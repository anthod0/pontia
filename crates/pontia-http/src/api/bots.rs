use std::convert::Infallible;

use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
    response::sse::{Event, KeepAlive, Sse},
};
use pontia_agent_control::{agent_clients::AgentClientService, client_contract::DetectionContext};
use pontia_bot::{
    conversation::{ConversationUpdate, NOTICE_EVENTS, ReplyRequest},
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
        .filter(|client| client.enabled && state.bots().supports_bot_client(&client.client_type))
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
    if !state.bots().supports_bot_client(&request.client_type) {
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
pub struct TimelineQuery {
    before: Option<String>,
    around: Option<String>,
    limit: Option<u32>,
}

pub async fn timeline(
    State(state): State<HttpState>,
    Path(bot_id): Path<String>,
    query: Result<Query<TimelineQuery>, QueryRejection>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let Query(query) = query.map_err(|error| ApiError::invalid_request(error.body_text()))?;
    let page = state
        .bots()
        .timeline(
            &bot_id,
            query.before.as_deref(),
            query.around.as_deref(),
            query.limit.unwrap_or(50),
        )
        .await?;
    Ok(ok(json!(page)))
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

#[derive(Default, Deserialize)]
pub struct StreamOptions {
    #[serde(default)]
    timeline: bool,
}

pub async fn stream(
    Query(options): Query<StreamOptions>,
    State(state): State<HttpState>,
    Path(bot_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    state.bots().records().get(&bot_id).await?;
    let mut updates = state.bots().subscribe(&bot_id);
    let mut events = state.app().control().agent_events().subscribe();
    let stream = async_stream::stream! {
        yield Ok(update_event(ConversationUpdate::Reset, options.timeline));
        loop {
            tokio::select! {
                update = updates.recv() => match update {
                    Ok(update) => yield Ok(update_event(update, options.timeline)),
                    Err(RecvError::Lagged(_)) => {
                        updates = updates.resubscribe();
                        events = events.resubscribe();
                        yield Ok(update_event(ConversationUpdate::Reset, options.timeline));
                    }
                    Err(RecvError::Closed) => break,
                },
                event = events.recv() => match event {
                    Ok(event) if matches!(
                        event.event_type,
                        EventType::TurnCreated
                            | EventType::TurnQueued
                            | EventType::TurnStarted
                            | EventType::TurnCompleted
                            | EventType::TurnFailed
                            | EventType::TurnDispatchFailed
                            | EventType::TurnAbandoned
                            | EventType::TurnInterrupted
                            | EventType::SessionError
                            | EventType::SessionExited
                    ) => {
                        match state.bots().records().get(&bot_id).await {
                            Ok(bot) if bot.current_session_id.as_deref() == Some(&event.session_id) =>
                            {
                                let update = if NOTICE_EVENTS.contains(&event.event_type) {
                                    ConversationUpdate::Reset
                                } else {
                                    ConversationUpdate::Changed
                                };
                                yield Ok(update_event(update, options.timeline));
                            }
                            Ok(_) => (),
                            Err(error) => {
                                tracing::warn!(%bot_id, %error, "Bot event stream lookup failed");
                                break;
                            }
                        }
                    }
                    Ok(_) => (),
                    Err(RecvError::Lagged(_)) => {
                        updates = updates.resubscribe();
                        events = events.resubscribe();
                        yield Ok(update_event(ConversationUpdate::Reset, options.timeline));
                    }
                    Err(RecvError::Closed) => break,
                },
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn update_event(update: ConversationUpdate, timeline: bool) -> Event {
    let data = if timeline {
        Event::default().json_data(update)
    } else {
        Event::default().json_data(match update {
            ConversationUpdate::Typing => "typing",
            _ => "changed",
        })
    };
    data.expect("serializable conversation update")
}

/// Run one maintenance round without changing the hourly timer.
pub async fn dream(State(state): State<HttpState>) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let service = state.dreaming().clone();
    let session_id = tokio::spawn(async move { service.start().await })
        .await
        .map_err(|error| pontia_core::Error::Domain(error.to_string()))??;
    Ok(ok(json!({"session_id": session_id})))
}

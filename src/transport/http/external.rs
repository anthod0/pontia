use std::{convert::Infallible, time::Duration};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};

use crate::{
    application::{
        AppState, ArtifactContentService, ArtifactDiscoveryService, CreateSessionRequest,
        EventStreamScope, ExternalQueryService, RuntimeControlService, SessionCommandService,
        SubmitTurnRequest, TurnCommandService,
    },
    error::Error,
};

#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    data: Option<T>,
    meta: Value,
    error: Option<ApiErrorBody>,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    code: &'static str,
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    after: Option<String>,
}

pub async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSessionRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = SessionCommandService::new(state.db);
    let outcome = service.create_session(request, idempotency_key).await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn list_sessions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    let sessions = service.list_sessions().await?;
    Ok(ok(json!({ "sessions": sessions })))
}

pub async fn get_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    let session = service
        .get_session(&session_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("session {session_id} not found")))?;
    Ok(ok(json!({ "session": session })))
}

pub async fn submit_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Json(request): Json<SubmitTurnRequest>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = TurnCommandService::new(state.db);
    let outcome = service
        .submit_turn(&session_id, request, idempotency_key)
        .await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

pub async fn interrupt_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = RuntimeControlService::new(state.db);
    let outcome = service
        .interrupt_current_turn(&session_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn terminate_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = RuntimeControlService::new(state.db);
    let outcome = service
        .terminate_session(&session_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn restart_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = RuntimeControlService::new(state.db);
    let outcome = service
        .restart_session(&session_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn interrupt_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Response, ExternalApiError> {
    authenticate(&state, &headers)?;
    let idempotency_key = headers
        .get("Idempotency-Key")
        .and_then(|value| value.to_str().ok());
    let service = RuntimeControlService::new(state.db);
    let outcome = service
        .interrupt_turn(&session_id, &turn_id, idempotency_key)
        .await?;
    Ok((StatusCode::OK, ok(outcome.data)).into_response())
}

pub async fn list_turns(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    let turns = service.list_turns(&session_id).await?;
    Ok(ok(json!({ "turns": turns })))
}

pub async fn get_turn(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    let turn = service
        .get_turn(&session_id, &turn_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("turn {turn_id} not found")))?;
    Ok(ok(json!({ "turn": turn })))
}

pub async fn list_session_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    let events = service.list_session_events(&session_id).await?;
    Ok(ok(json!({ "events": events })))
}

pub async fn list_turn_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
) -> Result<Json<ApiResponse<Value>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db);
    ensure_session_exists(&service, &session_id).await?;
    service
        .get_turn(&session_id, &turn_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("turn {turn_id} not found")))?;
    let events = service.list_turn_events(&session_id, &turn_id).await?;
    Ok(ok(json!({ "events": events })))
}

pub async fn stream_session_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<EventStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db.clone());
    ensure_session_exists(&service, &session_id).await?;
    let after_rowid = match query.after.as_deref() {
        Some(after) => {
            service
                .resolve_event_cursor(
                    EventStreamScope::Session {
                        session_id: &session_id,
                    },
                    after,
                )
                .await?
        }
        None => 0,
    };
    let stream_once = is_test_stream_once(&headers);

    Ok(event_sse_stream(
        state,
        EventStreamTarget::Session { session_id },
        after_rowid,
        stream_once,
    ))
}

pub async fn stream_turn_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
    Query(query): Query<EventStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db.clone());
    ensure_session_exists(&service, &session_id).await?;
    service
        .get_turn(&session_id, &turn_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("turn {turn_id} not found")))?;
    let after_rowid = match query.after.as_deref() {
        Some(after) => {
            service
                .resolve_event_cursor(
                    EventStreamScope::Turn {
                        session_id: &session_id,
                        turn_id: &turn_id,
                    },
                    after,
                )
                .await?
        }
        None => 0,
    };
    let stream_once = is_test_stream_once(&headers);

    Ok(event_sse_stream(
        state,
        EventStreamTarget::Turn {
            session_id,
            turn_id,
        },
        after_rowid,
        stream_once,
    ))
}

#[derive(Debug, Clone)]
enum EventStreamTarget {
    Session { session_id: String },
    Turn { session_id: String, turn_id: String },
}

fn event_sse_stream(
    state: AppState,
    target: EventStreamTarget,
    after_rowid: i64,
    stream_once: bool,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (sender, receiver) = mpsc::channel(32);

    tokio::spawn(async move {
        let service = ExternalQueryService::new(state.db);
        let mut cursor = after_rowid;

        loop {
            let result = match &target {
                EventStreamTarget::Session { session_id } => {
                    service
                        .list_event_stream_items_after(
                            EventStreamScope::Session { session_id },
                            cursor,
                            100,
                        )
                        .await
                }
                EventStreamTarget::Turn {
                    session_id,
                    turn_id,
                } => {
                    service
                        .list_event_stream_items_after(
                            EventStreamScope::Turn {
                                session_id,
                                turn_id,
                            },
                            cursor,
                            100,
                        )
                        .await
                }
            };

            let Ok(items) = result else {
                break;
            };

            if items.is_empty() {
                if stream_once {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }

            for item in items {
                cursor = item.rowid;
                let event_id = item.event.event_id.clone();
                let event = Event::default()
                    .id(event_id)
                    .event("domain_event")
                    .json_data(item.event);
                let Ok(event) = event else {
                    break;
                };
                if sender.send(Ok(event)).await.is_err() {
                    return;
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(receiver)).keep_alive(KeepAlive::default())
}

fn is_test_stream_once(headers: &HeaderMap) -> bool {
    headers
        .get("x-llmparty-test-stream-once")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

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

async fn ensure_session_exists(
    service: &ExternalQueryService,
    session_id: &str,
) -> Result<(), ExternalApiError> {
    service
        .get_session(session_id)
        .await?
        .ok_or_else(|| ExternalApiError::not_found(format!("session {session_id} not found")))?;
    Ok(())
}

fn authenticate(state: &AppState, headers: &HeaderMap) -> Result<(), ExternalApiError> {
    let Some(expected) = &state.external_api_token else {
        return Err(ExternalApiError::authentication_failed(
            "external API token is not configured",
        ));
    };

    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected);

    if authorized {
        Ok(())
    } else {
        Err(ExternalApiError::authentication_failed(
            "missing or invalid bearer token",
        ))
    }
}

fn ok(data: Value) -> Json<ApiResponse<Value>> {
    Json(ApiResponse {
        data: Some(data),
        meta: json!({}),
        error: None,
    })
}

#[derive(Debug)]
pub struct ExternalApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ExternalApiError {
    fn authentication_failed(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "authentication_failed",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }

    fn state_conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "state_conflict",
            message: message.into(),
        }
    }

    fn capability_unavailable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "capability_unavailable",
            message: message.into(),
        }
    }
}

impl From<Error> for ExternalApiError {
    fn from(error: Error) -> Self {
        match error {
            Error::StateConflict(message) => Self::state_conflict(message),
            Error::CapabilityUnavailable(message) => Self::capability_unavailable(message),
            Error::NotFound(message) => Self::not_found(message),
            Error::Domain(message) => Self {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_request",
                message,
            },
            other => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                message: other.to_string(),
            },
        }
    }
}

impl IntoResponse for ExternalApiError {
    fn into_response(self) -> Response {
        let body = Json(ApiResponse::<Value> {
            data: None,
            meta: json!({}),
            error: Some(ApiErrorBody {
                code: self.code,
                message: self.message,
            }),
        });
        (self.status, body).into_response()
    }
}

use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};

use crate::application::{AppState, EventStreamScope, ExternalQueryService};

use super::common::{ExternalApiError, authenticate, ensure_session_exists};

#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    after: Option<String>,
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

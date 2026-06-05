use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::Deserialize;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};

use crate::application::{AppState, DashboardStreamCursor, EventStreamScope, ExternalQueryService};

use super::common::{ExternalApiError, authenticate, ensure_session_exists};

#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    after: Option<String>,
}

pub async fn stream_dashboard_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db.clone());
    let cursor = match query.after.as_deref() {
        Some(after) => service.parse_dashboard_stream_cursor(after)?,
        None => DashboardStreamCursor {
            session_rowid: 0,
            task_rowid: 0,
        },
    };
    let stream_once = is_test_stream_once(&headers);

    Ok(dashboard_sse_stream(state, cursor, stream_once))
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

fn dashboard_cursor_id(cursor: DashboardStreamCursor) -> String {
    format!(
        "session:{};task:{}",
        cursor.session_rowid, cursor.task_rowid
    )
}

fn dashboard_sse_stream(
    state: AppState,
    after_cursor: DashboardStreamCursor,
    stream_once: bool,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (sender, receiver) = mpsc::channel(32);

    tokio::spawn(async move {
        let mut shutdown = state.shutdown.subscribe();
        let service = ExternalQueryService::new(state.db);
        let mut cursor = after_cursor;

        loop {
            if *shutdown.borrow() {
                break;
            }

            let Ok(items) = service.list_dashboard_stream_items_after(cursor, 100).await else {
                break;
            };

            if items.is_empty() {
                if stream_once {
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {}
                    _ = shutdown.changed() => break,
                }
                continue;
            }

            for item in items {
                cursor = item.cursor;
                let event = Event::default()
                    .id(dashboard_cursor_id(cursor))
                    .event("dashboard_event")
                    .json_data(item.event);
                let Ok(event) = event else {
                    break;
                };
                tokio::select! {
                    result = sender.send(Ok(event)) => {
                        if result.is_err() {
                            return;
                        }
                    }
                    _ = shutdown.changed() => return,
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(receiver)).keep_alive(KeepAlive::default())
}

fn event_sse_stream(
    state: AppState,
    target: EventStreamTarget,
    after_rowid: i64,
    stream_once: bool,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (sender, receiver) = mpsc::channel(32);

    tokio::spawn(async move {
        let mut shutdown = state.shutdown.subscribe();
        let service = ExternalQueryService::new(state.db);
        let mut cursor = after_rowid;

        loop {
            if *shutdown.borrow() {
                break;
            }

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
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {}
                    _ = shutdown.changed() => break,
                }
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
                tokio::select! {
                    result = sender.send(Ok(event)) => {
                        if result.is_err() {
                            return;
                        }
                    }
                    _ = shutdown.changed() => return,
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(receiver)).keep_alive(KeepAlive::default())
}

fn is_test_stream_once(headers: &HeaderMap) -> bool {
    headers
        .get("x-pilotfy-test-stream-once")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

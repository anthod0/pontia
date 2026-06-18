use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};

use pontia_application::{AppState, EventView, ExternalQueryService, TaskEventView};
use pontia_core::{
    domain::EventType,
    error::{Error, Result as AppResult},
};

use super::{
    common::{ExternalApiError, authenticate},
    events::{EventStreamQuery, event_view_from_domain_event, is_test_stream_once},
};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind")]
enum DashboardStreamEvent {
    #[serde(rename = "session_event")]
    SessionEvent {
        id: String,
        occurred_at: String,
        event: EventView,
    },
    #[serde(rename = "task_event")]
    TaskEvent {
        id: String,
        occurred_at: String,
        event: TaskEventView,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DashboardStreamCursor {
    session_rowid: i64,
    task_rowid: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct DashboardStreamItem {
    cursor: DashboardStreamCursor,
    occurred_at: String,
    event: DashboardStreamEvent,
}

pub async fn stream_dashboard_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventStreamQuery>,
) -> std::result::Result<
    Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>,
    ExternalApiError,
> {
    authenticate(&state, &headers)?;
    let service = ExternalQueryService::new(state.db());
    let cursor = match query.after.as_deref() {
        Some(after) => parse_dashboard_stream_cursor(after)?,
        None => current_dashboard_stream_cursor(&service).await?,
    };
    let stream_once = is_test_stream_once(&headers);

    Ok(dashboard_sse_stream(state, cursor, stream_once))
}

async fn current_dashboard_stream_cursor(
    service: &ExternalQueryService,
) -> AppResult<DashboardStreamCursor> {
    Ok(DashboardStreamCursor {
        session_rowid: service.current_session_stream_rowid().await?,
        task_rowid: service.current_task_stream_rowid().await?,
    })
}

fn parse_dashboard_stream_cursor(cursor: &str) -> AppResult<DashboardStreamCursor> {
    let mut session_rowid = None;
    let mut task_rowid = None;
    for part in cursor.split(';') {
        let Some((name, value)) = part.split_once(':') else {
            return Err(invalid_dashboard_cursor(cursor));
        };
        let rowid = value
            .parse::<i64>()
            .map_err(|_| invalid_dashboard_cursor(cursor))?;
        match name {
            "session" => session_rowid = Some(rowid),
            "task" => task_rowid = Some(rowid),
            _ => return Err(invalid_dashboard_cursor(cursor)),
        }
    }
    Ok(DashboardStreamCursor {
        session_rowid: session_rowid.unwrap_or(0),
        task_rowid: task_rowid.unwrap_or(0),
    })
}

fn invalid_dashboard_cursor(cursor: &str) -> Error {
    Error::Domain(format!("dashboard cursor {cursor} is invalid"))
}

async fn list_dashboard_stream_items_after(
    service: &ExternalQueryService,
    cursor: DashboardStreamCursor,
    limit: i64,
) -> AppResult<Vec<DashboardStreamItem>> {
    let session_items = service
        .list_session_stream_items_after(cursor.session_rowid, limit)
        .await?;
    let task_items = service
        .list_task_event_stream_items_after(cursor.task_rowid, limit)
        .await?;

    let mut items = Vec::new();
    for item in session_items {
        let rowid = item.rowid;
        let event = item.event;
        let occurred_at = event.time.clone();
        items.push(DashboardStreamItem {
            cursor: DashboardStreamCursor {
                session_rowid: rowid,
                task_rowid: cursor.task_rowid,
            },
            occurred_at: occurred_at.clone(),
            event: DashboardStreamEvent::SessionEvent {
                id: event.event_id.clone(),
                occurred_at,
                event,
            },
        });
    }
    for item in task_items {
        let rowid = item.rowid;
        let event = item.event;
        let occurred_at = event.created_at.clone();
        items.push(DashboardStreamItem {
            cursor: DashboardStreamCursor {
                session_rowid: cursor.session_rowid,
                task_rowid: rowid,
            },
            occurred_at: occurred_at.clone(),
            event: DashboardStreamEvent::TaskEvent {
                id: event.event_id.clone(),
                occurred_at,
                event,
            },
        });
    }

    items.sort_by(|a, b| {
        a.occurred_at
            .cmp(&b.occurred_at)
            .then(a.cursor.session_rowid.cmp(&b.cursor.session_rowid))
            .then(a.cursor.task_rowid.cmp(&b.cursor.task_rowid))
    });
    items.truncate(limit as usize);
    let mut running = cursor;
    for item in &mut items {
        running.session_rowid = running.session_rowid.max(item.cursor.session_rowid);
        running.task_rowid = running.task_rowid.max(item.cursor.task_rowid);
        item.cursor = running;
    }
    Ok(items)
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
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let (sender, receiver) = mpsc::channel(32);

    tokio::spawn(async move {
        let mut shutdown = state.shutdown().subscribe();
        let service = ExternalQueryService::new(state.db());
        let mut volatile_events = state.volatile_events().subscribe();
        let mut cursor = after_cursor;

        loop {
            if *shutdown.borrow() {
                break;
            }

            let Ok(items) = list_dashboard_stream_items_after(&service, cursor, 100).await else {
                break;
            };

            if items.is_empty() {
                if stream_once {
                    break;
                }
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {}
                    _ = shutdown.changed() => break,
                    received = volatile_events.recv() => {
                        if let Ok(event) = received
                            && event.event_type == EventType::SessionMessageUpdated
                            && let Some(view) = event_view_from_domain_event(&event)
                        {
                            let stream_event = DashboardStreamEvent::SessionEvent {
                                id: view.event_id.clone(),
                                occurred_at: view.time.clone(),
                                event: view,
                            };
                            let event = Event::default()
                                .event("dashboard_event")
                                .json_data(stream_event);
                            let Ok(event) = event else {
                                break;
                            };
                            if sender.send(Ok(event)).await.is_err() {
                                return;
                            }
                        }
                    }
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

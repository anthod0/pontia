use std::{convert::Infallible, time::Duration};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};

use pontia_application::{AppState, EventView};
use pontia_core::error::{Error, Result as AppResult};

use super::{
    events::{EventStreamQuery, is_test_stream_once},
    response::ApiError,
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
}

pub async fn stream_dashboard_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<EventStreamQuery>,
) -> std::result::Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>, ApiError>
{
    let service = state.queries();
    let cursor = match query.after.as_deref() {
        Some(after) => parse_dashboard_stream_cursor(after)?,
        None => service.current_session_stream_rowid().await?,
    };
    let stream_once = is_test_stream_once(&headers);

    Ok(dashboard_sse_stream(state, cursor, stream_once))
}

fn parse_dashboard_stream_cursor(cursor: &str) -> AppResult<i64> {
    cursor
        .strip_prefix("session:")
        .and_then(|value| value.parse().ok())
        .filter(|rowid| *rowid >= 0)
        .ok_or_else(|| Error::Domain(format!("dashboard cursor {cursor} is invalid")))
}

fn interaction_stream_event(
    change: Result<
        pontia_agent_control::interactions::InteractionChanged,
        tokio::sync::broadcast::error::RecvError,
    >,
) -> Option<Event> {
    let data = match change {
        Ok(change) => {
            serde_json::json!({"kind":"interaction_changed", "session_id":change.session_id, "interaction_id":change.interaction_id})
        }
        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
            serde_json::json!({"kind":"interactions_resync"})
        }
        Err(_) => return None,
    };
    Event::default()
        .event("dashboard_event")
        .json_data(data)
        .ok()
}

fn dashboard_sse_stream(
    state: AppState,
    after_cursor: i64,
    stream_once: bool,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let (sender, receiver) = mpsc::channel(32);
    let mut interactions = state.control().interactions().subscribe();
    let mut history = state.control().agent_events().subscribe_history();

    tokio::spawn(async move {
        let mut shutdown = state.control().shutdown().subscribe();
        let service = state.queries();
        let mut cursor = after_cursor;

        loop {
            if *shutdown.borrow() {
                break;
            }

            // A bounded transient batch between durable batches prevents either source
            // from starving the other. Transient frames never change the durable cursor.
            for _ in 0..32 {
                let change = match interactions.try_recv() {
                    Ok(change) => Ok(change),
                    Err(tokio::sync::broadcast::error::TryRecvError::Lagged(count)) => {
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(count))
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::broadcast::error::TryRecvError::Closed) => return,
                };
                let Some(event) = interaction_stream_event(change) else {
                    return;
                };
                tokio::select! {
                    result = sender.send(Ok(event)) => { if result.is_err() { return; } }
                    _ = shutdown.changed() => return,
                }
            }
            for _ in 0..32 {
                match history.try_recv() {
                    Ok(session_id) => {
                        let event = Event::default().event("dashboard_event").json_data(serde_json::json!({"kind":"history_changed", "session_id":session_id})).expect("history notification");
                        tokio::select! {
                            result = sender.send(Ok(event)) => { if result.is_err() { return; } }
                            _ = shutdown.changed() => return,
                        }
                    }
                    Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                    Err(_) => return,
                }
            }
            let Ok(items) = service.list_session_stream_items_after(cursor, 100).await else {
                break;
            };

            if items.is_empty() {
                if stream_once {
                    break;
                }
                tokio::select! {
                    change = history.recv() => {
                        let Ok(session_id) = change else { break; };
                        let event = Event::default().event("dashboard_event").json_data(serde_json::json!({"kind":"history_changed", "session_id":session_id})).expect("history notification");
                        tokio::select! {
                            result = sender.send(Ok(event)) => { if result.is_err() { break; } }
                            _ = shutdown.changed() => break,
                        }
                    }
                    change = interactions.recv() => {
                        if let Some(event) = interaction_stream_event(change) {
                            tokio::select! {
                                result = sender.send(Ok(event)) => { if result.is_err() { break; } }
                                _ = shutdown.changed() => break,
                            }
                        }
                    }
                    _ = sender.closed() => break,
                    _ = tokio::time::sleep(Duration::from_millis(200)) => {}
                    _ = shutdown.changed() => break,
                }
                continue;
            }

            for item in items {
                cursor = item.rowid;
                let event = Event::default()
                    .id(format!("session:{cursor}"))
                    .event("dashboard_event")
                    .json_data(DashboardStreamEvent::SessionEvent {
                        id: item.event.fact.event_id.clone(),
                        occurred_at: item.event.fact.time.clone(),
                        event: item.event,
                    });
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

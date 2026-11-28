use std::convert::Infallible;

use axum::{
    extract::{Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use pontia_agent_control::EventScope;
use serde::Serialize;
use tokio_stream::Stream;

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
    let service = state.control().events();
    let cursor = match query.after.as_deref() {
        Some(after) => parse_dashboard_stream_cursor(after)?,
        None => service.current_head_position().await?,
    };
    let stream_once = is_test_stream_once(&headers);

    Ok(dashboard_sse_stream(state, cursor, stream_once))
}

fn parse_dashboard_stream_cursor(cursor: &str) -> AppResult<i64> {
    cursor
        .strip_prefix("session:")
        .and_then(|value| value.parse().ok())
        .filter(|position| *position >= 0)
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
    let events = state.control().events();
    let mut interactions = state.control().interactions().subscribe();
    let mut history = events.subscribe_history();
    let mut shutdown = state.control().shutdown().subscribe();
    let stream = async_stream::stream! {
        let mut tail = state.control().events().tail(EventScope::All, after_cursor, stream_once);
        'tail: while !*shutdown.borrow() {
            let next = tail.next();
            tokio::pin!(next);
            loop {
                tokio::select! {
                    _ = shutdown.changed() => break 'tail,
                    change = interactions.recv() => {
                        let Some(event) = interaction_stream_event(change) else { break 'tail };
                        yield Ok(event);
                    }
                    change = history.recv() => {
                        let Ok(session_id) = change else { break 'tail };
                        yield Ok(Event::default().event("dashboard_event").json_data(
                            serde_json::json!({"kind":"history_changed", "session_id":session_id})
                        ).expect("history notification"));
                    }
                    item = &mut next => {
                        let Ok(Some(item)) = item else { break 'tail };
                        let position = item.position;
                        let Ok(event) = EventView::try_from(item) else { break 'tail };
                        let Ok(frame) = Event::default()
                            .id(format!("session:{position}"))
                            .event("dashboard_event")
                            .json_data(DashboardStreamEvent::SessionEvent {
                                id: event.event_id.clone(),
                                occurred_at: event.time.clone(),
                                event,
                            }) else { break 'tail };
                        yield Ok(frame);
                        break;
                    }
                }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

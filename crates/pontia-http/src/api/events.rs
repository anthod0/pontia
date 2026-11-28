use std::convert::Infallible;

use axum::{
    extract::{Path, Query, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
pub(super) use serde::Deserialize;
use tokio_stream::Stream;

use pontia_agent_control::{EventScope, EventView};
use pontia_application::AppState;

use super::{response::ApiError, session_guard::ensure_session_exists};

#[derive(Debug, Deserialize)]
pub struct EventStreamQuery {
    pub(super) after: Option<String>,
}

pub async fn stream_session_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
    Query(query): Query<EventStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    ensure_session_exists(&state.control().queries(), &session_id).await?;
    event_sse_stream(
        state,
        EventScope::Session(session_id),
        query,
        is_test_stream_once(&headers),
    )
    .await
}

pub async fn stream_turn_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((session_id, turn_id)): Path<(String, String)>,
    Query(query): Query<EventStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    ensure_session_exists(&state.control().queries(), &session_id).await?;
    state
        .control()
        .queries()
        .get_turn(&session_id, &turn_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("turn {turn_id} not found")))?;
    event_sse_stream(
        state,
        EventScope::Turn {
            session_id,
            turn_id,
        },
        query,
        is_test_stream_once(&headers),
    )
    .await
}

async fn event_sse_stream(
    state: AppState,
    scope: EventScope,
    query: EventStreamQuery,
    stream_once: bool,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let events = state.control().events();
    let after = match query.after {
        Some(id) => events
            .resolve_event_position(&scope, &id)
            .await?
            .ok_or_else(|| {
                ApiError::invalid_request(format!(
                    "event cursor {id} is not valid for requested stream"
                ))
            })?,
        None => 0,
    };
    let mut shutdown = state.control().shutdown().subscribe();
    let stream = async_stream::stream! {
        let mut tail = state.control().events().tail(scope, after, stream_once);
        while !*shutdown.borrow() {
            let item = tokio::select! {
                item = tail.next() => item,
                _ = shutdown.changed() => break,
            };
            let Ok(Some(item)) = item else { break };
            let Ok(view) = EventView::try_from(item) else { break };
            let Ok(event) = Event::default().id(&view.event_id).event("domain_event").json_data(&view) else { break };
            yield Ok(event);
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub(super) fn is_test_stream_once(headers: &HeaderMap) -> bool {
    headers
        .get("x-pontia-test-stream-once")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

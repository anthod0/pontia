use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::sse::{Event, KeepAlive, Sse},
};
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};

use pontia_application::{
    AppState, ExternalQueryService, LiveOutputSnapshot, LiveOutputStreamEvent,
};

use super::{
    authentication::authenticate, response::ExternalApiError, session_guard::ensure_session_exists,
};

pub async fn stream_live_output(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(session_id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ExternalApiError> {
    authenticate(&state, &headers)?;
    ensure_session_exists(&ExternalQueryService::new(state.db()), &session_id).await?;

    let subscription = state.live_output().subscribe_session(&session_id);
    Ok(live_output_sse_stream(state, session_id, subscription))
}

fn live_output_sse_stream(
    state: AppState,
    session_id: String,
    mut subscription: pontia_application::LiveOutputSubscription,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (sender, receiver) = mpsc::channel(32);

    tokio::spawn(async move {
        let mut shutdown = state.shutdown().subscribe();
        if let Some(snapshot) = subscription.initial_snapshot.take()
            && send_snapshot(&sender, snapshot).await.is_err()
        {
            return;
        }

        loop {
            tokio::select! {
                _ = shutdown.changed() => return,
                received = subscription.recv() => {
                    match received {
                        Ok(event) if event_session_id(&event) == session_id => {
                            let event_name = match &event {
                                LiveOutputStreamEvent::Snapshot { .. } => "snapshot",
                                LiveOutputStreamEvent::Updates { .. } => "updates",
                                LiveOutputStreamEvent::Closed { .. } => "closed",
                            };
                            let Ok(encoded) = Event::default().event(event_name).json_data(event) else {
                                return;
                            };
                            if sender.send(Ok(encoded)).await.is_err() {
                                return;
                            }
                        }
                        Ok(_) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            subscription = state.live_output().subscribe_session(&session_id);
                            if let Some(snapshot) = subscription.initial_snapshot.take()
                                && send_snapshot(&sender, snapshot).await.is_err()
                            {
                                return;
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    }
                }
            }
        }
    });

    Sse::new(ReceiverStream::new(receiver)).keep_alive(KeepAlive::default())
}

async fn send_snapshot(
    sender: &mpsc::Sender<Result<Event, Infallible>>,
    snapshot: LiveOutputSnapshot,
) -> Result<(), ()> {
    let event = LiveOutputStreamEvent::Snapshot { snapshot };
    let encoded = Event::default()
        .event("snapshot")
        .json_data(event)
        .map_err(|_| ())?;
    sender.send(Ok(encoded)).await.map_err(|_| ())
}

fn event_session_id(event: &LiveOutputStreamEvent) -> &str {
    match event {
        LiveOutputStreamEvent::Snapshot { snapshot } => &snapshot.identity.session_id,
        LiveOutputStreamEvent::Updates { identity, .. }
        | LiveOutputStreamEvent::Closed { identity, .. } => &identity.session_id,
    }
}

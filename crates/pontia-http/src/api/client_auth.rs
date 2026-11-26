//! Agent Client login sessions. Request bodies may carry secrets: they are never
//! logged, and parse errors never echo their content.

use std::{collections::BTreeMap, convert::Infallible};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderValue, StatusCode, header},
    response::{
        Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use pontia_agent_control::client_contract::auth::{
    AuthInput, AuthSessionSnapshot, StartLoginParams,
};
use pontia_application::AppState;
use pontia_core::Error;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{broadcast::error::RecvError, mpsc};
use tokio_stream::{Stream, wrappers::ReceiverStream};

use super::response::{ApiError, ApiResponse, ok};

type ClientPath = Path<(String, String)>;
type SessionPath = Path<(String, String, String)>;
type ApiResult = Result<Json<ApiResponse<Value>>, ApiError>;

pub(crate) async fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn api_error(error: Error) -> ApiError {
    match error {
        Error::Conflict {
            code: code @ "auth_capacity_exceeded",
            message,
        } => ApiError::custom(StatusCode::TOO_MANY_REQUESTS, code, message),
        other => other.into(),
    }
}

fn session(snapshot: AuthSessionSnapshot) -> Json<ApiResponse<Value>> {
    ok(json!({ "session": snapshot }))
}

pub async fn get_client_auth(
    State(state): State<AppState>,
    Path((client, key)): ClientPath,
) -> ApiResult {
    let status = state
        .control()
        .client_auth()
        .status(&client, &key)
        .await
        .map_err(api_error)?;
    Ok(ok(json!({ "auth": status })))
}

pub async fn probe_client_auth(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Value>>, ApiError> {
    let probe = state
        .control()
        .client_auth()
        .probe_statuses_once()
        .await
        .map_err(api_error)?;
    Ok(ok(json!({ "auth_probe": probe })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartRequest {
    method_id: String,
    #[serde(default)]
    fields: BTreeMap<String, String>,
}

pub async fn start_client_auth_session(
    State(state): State<AppState>,
    Path((client, key)): ClientPath,
    body: Bytes,
) -> ApiResult {
    let request: StartRequest = serde_json::from_slice(&body)
        .map_err(|_| ApiError::invalid_request("Expected method_id and optional string fields"))?;
    let snapshot = state
        .control()
        .client_auth()
        .start_login(
            &client,
            &key,
            &request.method_id,
            StartLoginParams {
                fields: request.fields,
            },
        )
        .await
        .map_err(api_error)?;
    Ok(session(snapshot))
}

pub async fn get_client_auth_session(
    State(state): State<AppState>,
    Path((client, key, id)): SessionPath,
) -> ApiResult {
    let snapshot = state
        .control()
        .client_auth()
        .snapshot(&client, &key, &id)
        .map_err(api_error)?;
    Ok(session(snapshot))
}

pub async fn submit_client_auth_input(
    State(state): State<AppState>,
    Path((client, key, id)): SessionPath,
    body: Bytes,
) -> ApiResult {
    let input: AuthInput = serde_json::from_slice(&body)
        .map_err(|_| ApiError::invalid_request("Invalid login input"))?;
    let snapshot = state
        .control()
        .client_auth()
        .submit_input(&client, &key, &id, input)
        .await
        .map_err(api_error)?;
    Ok(session(snapshot))
}

pub async fn cancel_client_auth_session(
    State(state): State<AppState>,
    Path((client, key, id)): SessionPath,
) -> ApiResult {
    let snapshot = state
        .control()
        .client_auth()
        .cancel(&client, &key, &id)
        .await
        .map_err(api_error)?;
    Ok(session(snapshot))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogoutRequest {
    #[serde(default)]
    scope: Option<String>,
}

pub async fn logout_client_auth(
    State(state): State<AppState>,
    Path((client, key)): ClientPath,
    body: Bytes,
) -> ApiResult {
    let request = if body.iter().all(u8::is_ascii_whitespace) {
        LogoutRequest { scope: None }
    } else {
        serde_json::from_slice(&body)
            .map_err(|_| ApiError::invalid_request("Expected an optional string scope"))?
    };
    let status = state
        .control()
        .client_auth()
        .logout(&client, &key, request.scope.as_deref())
        .await
        .map_err(api_error)?;
    Ok(ok(json!({ "auth": status })))
}

/// `snapshot` first, then `updated` until a final `terminal` event.
pub async fn stream_client_auth_session(
    State(state): State<AppState>,
    Path((client, key, id)): SessionPath,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let service = state.control().client_auth();
    let (snapshot, mut events) = service.subscribe(&client, &key, &id).map_err(api_error)?;
    let (sender, receiver) = mpsc::channel(16);
    tokio::spawn(async move {
        let mut shutdown = state.control().shutdown().subscribe();
        let mut last = snapshot.sequence;
        if send(&sender, "snapshot", &snapshot).await.is_err() || snapshot.state.is_terminal() {
            return;
        }
        loop {
            let (name, snapshot) = tokio::select! {
                _ = shutdown.changed() => return,
                received = events.recv() => match received {
                    Ok(snapshot) if snapshot.sequence > last => ("updated", snapshot),
                    Ok(_) => continue,
                    Err(RecvError::Lagged(_)) => {
                        let Ok(snapshot) = service.snapshot(&client, &key, &id) else { return };
                        ("snapshot", snapshot)
                    }
                    Err(RecvError::Closed) => return,
                },
            };
            last = snapshot.sequence;
            let terminal = snapshot.state.is_terminal();
            let name = if terminal { "terminal" } else { name };
            if send(&sender, name, &snapshot).await.is_err() || terminal {
                return;
            }
        }
    });
    Ok(Sse::new(ReceiverStream::new(receiver)).keep_alive(KeepAlive::default()))
}

async fn send(
    sender: &mpsc::Sender<Result<Event, Infallible>>,
    name: &str,
    snapshot: &AuthSessionSnapshot,
) -> Result<(), ()> {
    let event = Event::default()
        .event(name)
        .id(snapshot.sequence.to_string())
        .json_data(snapshot)
        .map_err(|_| ())?;
    sender.send(Ok(event)).await.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use axum::response::IntoResponse;

    use super::*;

    #[test]
    fn login_capacity_maps_to_too_many_requests() {
        let conflict = |code| Error::Conflict {
            code,
            message: String::new(),
        };
        assert_eq!(
            api_error(conflict("auth_capacity_exceeded"))
                .into_response()
                .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            api_error(conflict("auth_session_in_progress"))
                .into_response()
                .status(),
            StatusCode::CONFLICT
        );
    }
}

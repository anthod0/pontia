use std::convert::Infallible;

use axum::{Json, body::Body, extract::State, response::IntoResponse};
use pontia_agent_control::runtime::shutdown::{Shutdown, ShutdownRequest};
use pontia_application::AppState;
use tokio_stream::StreamExt;

use super::ApiError;

pub(crate) async fn shutdown_agents(
    State(state): State<AppState>,
    Json(request): Json<ShutdownRequest>,
) -> Result<impl IntoResponse, ApiError> {
    if state.pontia_home() != request.pontia_home
        && request.pontia_home.canonicalize().ok().as_deref()
            != Some(
                &state
                    .pontia_home()
                    .canonicalize()
                    .map_err(pontia_core::Error::from)?,
            )
    {
        return Err(ApiError::custom(
            axum::http::StatusCode::CONFLICT,
            "different_instance",
            "A different Pontia instance is listening at this address",
        ));
    }
    let control = state.control();
    let shutdown = Shutdown::inspect(state.pontia_home(), &control.clients(), async {
        Ok(Some(state.db()))
    })
    .await
    .map_err(|error| {
        ApiError::custom(
            axum::http::StatusCode::CONFLICT,
            "shutdown_failed",
            error.to_string(),
        )
    })?;
    let updates = shutdown
        .execute(request)
        .map_err(|message| {
            ApiError::custom(axum::http::StatusCode::CONFLICT, "shutdown_failed", message)
        })?
        .map(|update| {
            let mut line = serde_json::to_vec(&update).expect("serializable shutdown update");
            line.push(b'\n');
            Ok::<_, Infallible>(line)
        });
    Ok((
        [(axum::http::header::CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(updates),
    ))
}

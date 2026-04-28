use axum::{
    Router,
    routing::{get, post},
};

use crate::application::AppState;

pub mod external;
pub mod health;
pub mod internal;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/internal/v1/events", post(internal::post_event))
        .route(
            "/external/v1/sessions",
            get(external::list_sessions).post(external::create_session),
        )
        .route(
            "/external/v1/sessions/{session_id}",
            get(external::get_session).delete(external::terminate_session),
        )
        .route(
            "/external/v1/sessions/{session_id}/interrupt",
            post(external::interrupt_session),
        )
        .route(
            "/external/v1/sessions/{session_id}/restart",
            post(external::restart_session),
        )
        .route(
            "/external/v1/sessions/{session_id}/turns",
            get(external::list_turns).post(external::submit_turn),
        )
        .route(
            "/external/v1/sessions/{session_id}/turns/{turn_id}",
            get(external::get_turn),
        )
        .route(
            "/external/v1/sessions/{session_id}/turns/{turn_id}/interrupt",
            post(external::interrupt_turn),
        )
        .route(
            "/external/v1/sessions/{session_id}/events/stream",
            get(external::stream_session_events),
        )
        .route(
            "/external/v1/sessions/{session_id}/events",
            get(external::list_session_events),
        )
        .route(
            "/external/v1/sessions/{session_id}/turns/{turn_id}/events/stream",
            get(external::stream_turn_events),
        )
        .route(
            "/external/v1/sessions/{session_id}/turns/{turn_id}/events",
            get(external::list_turn_events),
        )
        .route(
            "/external/v1/sessions/{session_id}/artifacts",
            get(external::list_artifacts),
        )
        .route(
            "/external/v1/sessions/{session_id}/artifacts/discover",
            post(external::discover_artifacts),
        )
        .route(
            "/external/v1/artifacts/{artifact_id}",
            get(external::get_artifact),
        )
        .route(
            "/external/v1/artifacts/{artifact_id}/content",
            get(external::get_artifact_content),
        )
        .with_state(state)
}

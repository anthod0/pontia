use axum::{
    Router,
    routing::{get, post},
};

use crate::application::AppState;

pub mod health;
pub mod internal;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/internal/v1/events", post(internal::post_event))
        .with_state(state)
}

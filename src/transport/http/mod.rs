use axum::{Router, routing::get};

pub mod health;

pub fn router() -> Router {
    Router::new().route("/healthz", get(health::healthz))
}

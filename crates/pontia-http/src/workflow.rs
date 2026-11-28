use crate::api;
use axum::{
    Router, middleware,
    routing::{get, post},
};
use pontia_application::AppState;

pub fn workflow_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/v1/workflow/submissions",
            post(api::submit_workflow_output),
        )
        .route(
            "/api/v1/workflow/terminate",
            post(api::terminate_workflow_node),
        )
        .route(
            "/api/v1/workflows",
            get(api::list_workflows).post(api::run_workflow),
        )
        .route("/api/v1/workflows/{workflow_id}", get(api::get_workflow))
        .route(
            "/api/v1/workflows/{workflow_id}/context",
            get(api::get_workflow_context),
        )
        .route(
            "/api/v1/workflows/{workflow_id}/timeline",
            get(api::get_workflow_timeline),
        )
        .route(
            "/api/v1/workflows/{workflow_id}/pause",
            post(api::pause_workflow),
        )
        .route(
            "/api/v1/workflows/{workflow_id}/resume",
            post(api::resume_workflow),
        )
        .route(
            "/api/v1/workflows/{workflow_id}/retry",
            post(api::retry_workflow),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            api::authenticate,
        ))
        .with_state(state)
}

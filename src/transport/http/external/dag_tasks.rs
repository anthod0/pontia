use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};

use crate::application::{AppState, CreateDagTaskRequest, TaskCommandService};

use super::common::{ExternalApiError, authenticate, idempotency_key, ok};

pub async fn create_dag_task(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateDagTaskRequest>,
) -> Result<Response, ExternalApiError> {
    // DAG task development is currently frozen. Keep this API behavior intact for
    // existing callers/tests and future revival, but avoid extending this path while
    // product focus is session-first Web UI and bidirectional session control.
    authenticate(&state, &headers)?;
    let idempotency_key = idempotency_key(&headers);
    let service = TaskCommandService::with_runtime(state.db, state.graph);
    let outcome = service.create_dag_task(request, idempotency_key).await?;
    let status = if outcome.duplicate {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, ok(outcome.data)).into_response())
}

use std::{
    future::{Future, IntoFuture},
    time::Duration,
};

use axum::{
    Router,
    routing::{get, post},
};
use tokio::sync::oneshot;
use tracing::warn;

use crate::{application::AppState, error::Result};

pub mod dashboard;
pub mod external;
pub mod health;
pub mod internal;
pub mod internal_agent_tools;

pub async fn serve_with_shutdown_timeout<F>(
    listener: tokio::net::TcpListener,
    router: Router,
    shutdown: F,
    shutdown_timeout: Duration,
) -> Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let (shutdown_started_tx, shutdown_started_rx) = oneshot::channel::<()>();
    let server = axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            shutdown.await;
            let _ = shutdown_started_tx.send(());
        })
        .into_future();
    tokio::pin!(server);

    tokio::select! {
        result = &mut server => {
            result?;
        }
        _ = shutdown_started_rx => {
            match tokio::time::timeout(shutdown_timeout, &mut server).await {
                Ok(result) => {
                    result?;
                }
                Err(_) => {
                    warn!(timeout_ms = shutdown_timeout.as_millis(), "graceful shutdown timed out; forcing server stop");
                }
            }
        }
    }

    Ok(())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/dashboard", get(dashboard::dashboard))
        .route("/dashboard/", get(dashboard::dashboard))
        .route("/dashboard/assets/{*path}", get(dashboard::dashboard_asset))
        .route("/dashboard/{*path}", get(dashboard::dashboard))
        .route("/internal/v1/events", post(internal::post_event))
        .route(
            "/internal/v1/agent-tools/{tool_name}",
            post(internal_agent_tools::post_agent_tool),
        )
        .route(
            "/external/v1/sessions",
            get(external::list_sessions).post(external::create_session),
        )
        .route(
            "/external/v1/agent-profiles",
            get(external::list_agent_profiles).post(external::create_agent_profile),
        )
        .route(
            "/external/v1/agent-profiles/{profile_id}",
            get(external::get_agent_profile).delete(external::delete_agent_profile),
        )
        .route(
            "/external/v1/agent-profiles/{profile_id}/versions",
            get(external::list_agent_profile_versions).post(external::create_agent_profile_version),
        )
        .route(
            "/external/v1/agent-profiles/{profile_id}/versions/{version}",
            get(external::get_agent_profile_version)
                .put(external::update_agent_profile_version)
                .delete(external::delete_agent_profile_version),
        )
        .route(
            "/external/v1/workspaces",
            get(external::list_workspaces).post(external::register_workspace),
        )
        .route(
            "/external/v1/workspaces/{workspace_id}",
            get(external::get_workspace).delete(external::delete_workspace),
        )
        .route(
            "/external/v1/workspace-roots",
            get(external::list_workspace_roots),
        )
        .route(
            "/external/v1/workspace-roots/{root_id}/entries",
            get(external::list_workspace_root_entries),
        )
        .route(
            "/external/v1/tasks",
            get(external::list_tasks).post(external::create_task),
        )
        .route(
            "/external/v1/dashboard/events/stream",
            get(external::stream_dashboard_events),
        )
        .route("/external/v1/dag-tasks", post(external::create_dag_task))
        .route("/external/v1/tasks/{task_id}", get(external::get_task))
        .route(
            "/external/v1/tasks/{task_id}/confirm-workspace",
            post(external::confirm_task_workspace),
        )
        .route(
            "/external/v1/tasks/{task_id}/planner-input",
            post(external::submit_planner_input),
        )
        .route(
            "/external/v1/tasks/{task_id}/events",
            get(external::list_task_events),
        )
        .route(
            "/external/v1/tasks/{task_id}/dag",
            get(external::get_task_dag),
        )
        .route(
            "/external/v1/tasks/{task_id}/work-items",
            get(external::list_task_work_items),
        )
        .route(
            "/external/v1/tasks/{task_id}/work-item-runs",
            get(external::list_task_work_item_runs),
        )
        .route(
            "/external/v1/tasks/{task_id}/signals",
            get(external::list_task_signals).post(external::create_human_signal),
        )
        .route(
            "/external/v1/tasks/{task_id}/scheduler/tick",
            post(external::scheduler_tick),
        )
        .route(
            "/external/v1/tasks/{task_id}/provenance",
            get(external::get_task_provenance),
        )
        .route(
            "/external/v1/tasks/{task_id}/pause",
            post(external::pause_task),
        )
        .route(
            "/external/v1/tasks/{task_id}/resume",
            post(external::resume_task),
        )
        .route(
            "/external/v1/tasks/{task_id}/interrupt",
            post(external::interrupt_task),
        )
        .route(
            "/external/v1/tasks/{task_id}/cancel",
            post(external::cancel_task),
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
            get(external::list_turns),
        )
        .route(
            "/external/v1/sessions/{session_id}/inbox/messages",
            get(external::list_inbox_messages).post(external::submit_inbox_message),
        )
        .route(
            "/external/v1/sessions/{session_id}/inbox/messages/{message_id}",
            get(external::get_inbox_message),
        )
        .route(
            "/external/v1/sessions/{session_id}/inbox/messages/{message_id}/cancel",
            post(external::cancel_inbox_message),
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

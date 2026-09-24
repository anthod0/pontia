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

use pontia_core::error::Result;

pub use state::HttpState;

pub mod api;
pub mod dashboard;
pub mod health;
pub mod state;

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

pub fn router(state: impl Into<HttpState>) -> Router {
    let state = state.into();
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/dashboard", get(dashboard::dashboard))
        .route("/dashboard/", get(dashboard::dashboard))
        .route("/dashboard/assets/{*path}", get(dashboard::dashboard_asset))
        .route("/dashboard/{*path}", get(dashboard::dashboard_path))
        .route(
            "/api/v1/workflow/submissions",
            post(api::submit_workflow_output),
        )
        .route(
            "/api/v1/workflow/patches/request",
            post(api::request_workflow_patch),
        )
        .route(
            "/api/v1/workflow/patches/apply",
            post(api::apply_workflow_patch),
        )
        .route(
            "/api/v1/workflow/patches/block",
            post(api::block_workflow_patch),
        )
        .route("/api/v1/auth/validate", get(api::validate_auth))
        .route(
            "/api/v1/sessions",
            get(api::list_sessions).post(api::create_session),
        )
        .route(
            "/api/v1/agent-profiles",
            get(api::list_agent_profiles).post(api::create_agent_profile),
        )
        .route(
            "/api/v1/agent-profiles/{profile_id}",
            get(api::get_agent_profile).delete(api::delete_agent_profile),
        )
        .route(
            "/api/v1/agent-profiles/{profile_id}/versions",
            get(api::list_agent_profile_versions).post(api::create_agent_profile_version),
        )
        .route(
            "/api/v1/agent-profiles/{profile_id}/versions/{version}",
            get(api::get_agent_profile_version)
                .put(api::update_agent_profile_version)
                .delete(api::delete_agent_profile_version),
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
            "/api/v1/workflows/{workflow_id}/revisions/{revision}",
            get(api::get_workflow_revision),
        )
        .route(
            "/api/v1/workflows/{workflow_id}/patches",
            get(api::list_workflow_patches),
        )
        .route(
            "/api/v1/workflows/{workflow_id}/timeline",
            get(api::get_workflow_timeline),
        )
        .route(
            "/api/v1/workflows/{workflow_id}/documents",
            get(api::get_workflow_document),
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
            "/api/v1/workspaces",
            get(api::list_workspaces).post(api::register_workspace),
        )
        .route(
            "/api/v1/workspaces/{workspace_id}/git-status",
            get(api::get_workspace_git_status),
        )
        .route(
            "/api/v1/workspaces/{workspace_id}/file-picker",
            get(api::pick_workspace_files),
        )
        .route(
            "/api/v1/workspaces/{workspace_id}/git-status/refresh",
            post(api::refresh_workspace_git_status),
        )
        .route(
            "/api/v1/workspaces/{workspace_id}",
            get(api::get_workspace)
                .patch(api::rename_workspace)
                .delete(api::delete_workspace),
        )
        .route("/api/v1/workspace-roots", get(api::list_workspace_roots))
        .route(
            "/api/v1/workspace-roots/{root_id}/entries",
            get(api::list_workspace_root_entries),
        )
        .route("/api/v1/tasks", get(api::list_tasks).post(api::create_task))
        .route(
            "/api/v1/dashboard/events/stream",
            get(api::stream_dashboard_events),
        )
        .route("/api/v1/tasks/{task_id}", get(api::get_task))
        .route("/api/v1/tasks/{task_id}/events", get(api::list_task_events))
        .route(
            "/api/v1/tasks/{task_id}/interrupt",
            post(api::interrupt_task),
        )
        .route("/api/v1/tasks/{task_id}/cancel", post(api::cancel_task))
        .route(
            "/api/v1/sessions/{session_id}",
            get(api::get_session)
                .patch(api::update_session)
                .delete(api::terminate_session),
        )
        .route(
            "/api/v1/sessions/{session_id}/models",
            get(api::list_session_models),
        )
        .route(
            "/api/v1/sessions/{session_id}/model",
            axum::routing::patch(api::set_session_model),
        )
        .route("/api/v1/sessions/{session_id}/pin", post(api::pin_session))
        .route(
            "/api/v1/sessions/{session_id}/unpin",
            post(api::unpin_session),
        )
        .route(
            "/api/v1/sessions/{session_id}/archive",
            post(api::archive_session),
        )
        .route(
            "/api/v1/sessions/{session_id}/unarchive",
            post(api::unarchive_session),
        )
        .route(
            "/api/v1/sessions/{session_id}/interrupt",
            post(api::interrupt_session),
        )
        .route(
            "/api/v1/sessions/{session_id}/restart",
            post(api::restart_session),
        )
        .route(
            "/api/v1/sessions/{session_id}/resume",
            post(api::resume_session),
        )
        .route(
            "/api/v1/sessions/{session_id}/tui",
            post(api::open_codex_tui),
        )
        // Read-only turn history. Direct turn dispatch via POST is intentionally not exposed:
        // Web input is submitted through the inbox API, and reported Agent facts own turn lifecycle.
        .route("/api/v1/sessions/{session_id}/turns", get(api::list_turns))
        .route(
            "/api/v1/sessions/{session_id}/turns/timeline",
            get(api::get_turn_timeline),
        )
        .route(
            "/api/v1/sessions/{session_id}/live-output/stream",
            get(api::stream_live_output),
        )
        .route(
            "/api/v1/sessions/{session_id}/turns/tree/history",
            get(api::get_turn_tree_history),
        )
        .route(
            "/api/v1/sessions/{session_id}/turns/tree/updates",
            get(api::get_turn_tree_updates),
        )
        .route(
            "/api/v1/sessions/{session_id}/inbox/messages",
            get(api::list_inbox_messages).post(api::submit_inbox_message),
        )
        .route(
            "/api/v1/sessions/{session_id}/inbox/messages/{message_id}",
            get(api::get_inbox_message).put(api::put_inbox_message),
        )
        .route(
            "/api/v1/sessions/{session_id}/inbox/messages/{message_id}/retry",
            post(api::retry_inbox_message),
        )
        .route(
            "/api/v1/sessions/{session_id}/inbox/messages/{message_id}/cancel",
            post(api::cancel_inbox_message),
        )
        .route(
            "/api/v1/sessions/{session_id}/inbox/messages/{message_id}/dismiss",
            post(api::dismiss_inbox_message),
        )
        .route(
            "/api/v1/sessions/{session_id}/turns/{turn_id}",
            get(api::get_turn),
        )
        .route(
            "/api/v1/sessions/{session_id}/turns/{turn_id}/interrupt",
            post(api::interrupt_turn),
        )
        .route(
            "/api/v1/sessions/{session_id}/events/stream",
            get(api::stream_session_events),
        )
        .route(
            "/api/v1/sessions/{session_id}/events",
            get(api::list_session_events),
        )
        .route(
            "/api/v1/sessions/{session_id}/turns/{turn_id}/events/stream",
            get(api::stream_turn_events),
        )
        .route(
            "/api/v1/sessions/{session_id}/turns/{turn_id}/events",
            get(api::list_turn_events),
        )
        .with_state(state)
}

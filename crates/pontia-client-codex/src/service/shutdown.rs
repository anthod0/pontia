use std::path::{Path, PathBuf};

use pontia_agent_control::{
    EventIngestService,
    client_contract::PreparedServiceShutdown,
    runtime::shutdown::{ShutdownSession, terminate_tree},
};
use pontia_core::Result;
use serde_json::json;
use sqlx::SqlitePool;

pub(super) async fn inspect(
    root: &Path,
    events: Option<&EventIngestService>,
) -> Result<PreparedServiceShutdown> {
    let server = crate::runtime::background_server(root).await?;
    let sessions = match events {
        Some(events) if !server.owned.is_empty() => background_sessions(&events.db()).await?,
        _ => Vec::new(),
    };
    let mut listing: Vec<_> = sessions.iter().map(ShutdownSession::label).collect();
    for process in &server.owned {
        listing.push(format!("Codex background app-server (PID {})", process.pid));
    }
    if let Some(listener) = &server.unverified_listener {
        listing.push(format!(
            "{} (ownership unverified)",
            listener_label(listener)
        ));
    }
    let events = events.cloned();
    let terminate = Box::pin(async move {
        for process in &server.owned {
            terminate_tree(process).await?;
        }
        if let Some(events) = events {
            for session in sessions {
                session
                    .exited(&events, json!({"reason":"management_exit"}))
                    .await?;
            }
        }
        Ok(server.unverified_listener.map(|listener| format!(
            "Warning: {} was left running because its ownership could not be verified. Inspect the listener with `lsof -U`, verify its PID and stop it manually with `kill -TERM <verified-PID>`.",
            listener_label(&listener)
        )))
    });
    Ok(PreparedServiceShutdown { listing, terminate })
}

async fn background_sessions(pool: &SqlitePool) -> Result<Vec<ShutdownSession>> {
    let sessions: Vec<ShutdownSession> = sqlx::query_as(
            "SELECT s.session_id, s.title, s.client_type, COALESCE(w.display_path, s.workspace_ref) AS workspace
             FROM sessions s LEFT JOIN workspaces w USING(workspace_id)
             WHERE s.client_type='codex' AND s.state NOT IN ('exited','error')
             ORDER BY s.session_id",
        ).fetch_all(pool).await?;
    let queries = pontia_agent_control::ControlQueryService::new(pool.clone());
    let mut background = Vec::new();
    for session in sessions {
        if queries.session_launch_mode(&session.session_id).await?
            == pontia_core::LaunchMode::Background
        {
            background.push(session);
        }
    }
    Ok(background)
}

fn listener_label((path, pid): &(PathBuf, Option<u32>)) -> String {
    let pid = pid.map(|pid| format!(", PID {pid}")).unwrap_or_default();
    format!("Codex background listener at {}{pid}", path.display())
}

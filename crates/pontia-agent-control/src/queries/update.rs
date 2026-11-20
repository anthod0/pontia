use super::ControlQueryService;
use pontia_core::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct UpdateBlocker {
    pub session_id: String,
    pub title: Option<String>,
    pub client_type: String,
    pub workspace_path: Option<String>,
    pub reason: String,
}

impl ControlQueryService {
    /// A snapshot of known execution activity, not a reservation of an idle window.
    pub async fn update_blockers(&self) -> Result<Vec<UpdateBlocker>> {
        Ok(sqlx::query_as(
            r#"SELECT b.session_id, s.title, s.client_type, w.display_path AS workspace_path, b.reason
               FROM (
                   SELECT session_id,
                          CASE state WHEN 'starting' THEN 'starting'
                                     WHEN 'busy' THEN 'running a task'
                                     ELSE 'waiting for your response' END AS reason
                   FROM sessions WHERE state IN ('starting', 'busy', 'waiting_for_interaction')
                   UNION
                   SELECT session_id, 'running a task'
                   FROM turns WHERE state = 'running'
                   UNION
                   SELECT session_id, 'has a queued task'
                   FROM turns WHERE state = 'queued'
                   UNION
                   SELECT session_id, 'waiting for your response'
                   FROM interactions WHERE state IN ('pending', 'resolving')
                   UNION
                   SELECT session_id, 'has messages not yet delivered'
                   FROM inbox_messages
                   WHERE state IN ('pending', 'resuming', 'dispatching')
                      OR (state IN ('dispatched', 'unknown') AND turn_id IS NULL)
               ) b
               JOIN sessions s USING (session_id)
               LEFT JOIN workspaces w ON w.workspace_id = s.workspace_id
               ORDER BY s.updated_at DESC, b.session_id, b.reason"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }
}

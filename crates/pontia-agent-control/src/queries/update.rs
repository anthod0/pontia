use super::ControlQueryService;
use pontia_core::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, sqlx::FromRow)]
pub struct UpdateBlocker {
    pub session_id: String,
    pub reason: String,
}

impl ControlQueryService {
    /// A snapshot of known execution activity, not a reservation of an idle window.
    pub async fn update_blockers(&self) -> Result<Vec<UpdateBlocker>> {
        Ok(sqlx::query_as(
            r#"SELECT session_id, 'Session is starting or executing' AS reason
               FROM sessions WHERE state IN ('starting', 'busy', 'waiting_for_interaction')
               UNION
               SELECT session_id, 'Turn is queued or running'
               FROM turns WHERE state IN ('queued', 'running')
               UNION
               SELECT session_id, 'Interaction is pending or resolving'
               FROM interactions WHERE state IN ('pending', 'resolving')
               UNION
               SELECT session_id, 'Input is pending, in flight, or unconfirmed'
               FROM inbox_messages
               WHERE state IN ('pending', 'resuming', 'dispatching')
                  OR (state IN ('dispatched', 'unknown') AND turn_id IS NULL)
               ORDER BY session_id, reason"#,
        )
        .fetch_all(&self.pool)
        .await?)
    }
}

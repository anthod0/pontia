use crate::ControlQueryService;
use pontia_core::{Error, Result, execution_policy::ExecutionPolicy};

impl ControlQueryService {
    pub async fn session_execution_policy(&self, session: &str) -> Result<ExecutionPolicy> {
        // Forks inherit the immutable launch policy of their source Session.
        let policy: Option<String> = sqlx::query_scalar(
            "WITH RECURSIVE ancestors(session_id, depth) AS (
                 SELECT session_id, 0 FROM sessions WHERE session_id=?
                 UNION ALL
                 SELECT l.parent_session_id, a.depth+1 FROM session_lineage l
                 JOIN ancestors a ON l.child_session_id=a.session_id
             )
             SELECT json_extract(e.payload, '$.execution_policy') FROM ancestors a
             JOIN events e ON e.session_id=a.session_id AND e.event_type='session.created'
             ORDER BY (json_type(e.payload, '$.execution_policy') IS NULL), a.depth
             LIMIT 1",
        )
        .bind(session)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Session {session} not found")))?;
        Ok(policy
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default())
    }
}

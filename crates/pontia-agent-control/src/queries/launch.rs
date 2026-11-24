use crate::ControlQueryService;
use pontia_core::{Error, Result, SessionMode};

impl ControlQueryService {
    pub async fn session_mode(&self, session: &str) -> Result<SessionMode> {
        self.launch_attribute(session, "$.mode").await
    }

    pub async fn session_launch_mode(&self, session: &str) -> Result<pontia_core::LaunchMode> {
        self.launch_attribute(session, "$.launch_mode").await
    }

    async fn launch_attribute<T: serde::de::DeserializeOwned + Default>(
        &self,
        session: &str,
        path: &str,
    ) -> Result<T> {
        // Keep the small ancestor set outermost so events use the Session index.
        let attribute: Option<String> = sqlx::query_scalar(
            "WITH RECURSIVE ancestors(session_id, depth) AS (
                 SELECT session_id, 0 FROM sessions WHERE session_id=?
                 UNION ALL
                 SELECT l.parent_session_id, a.depth+1 FROM session_lineage l
                 JOIN ancestors a ON l.child_session_id=a.session_id
             )
             SELECT e.payload -> ?2 FROM ancestors a
             CROSS JOIN events e ON e.session_id=a.session_id AND e.event_type='session.created'
             ORDER BY (json_type(e.payload, ?2) IS NULL), a.depth
             LIMIT 1",
        )
        .bind(session)
        .bind(path)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| Error::NotFound(format!("Session {session} not found")))?;
        Ok(attribute
            .map(|value| serde_json::from_str(&value))
            .transpose()?
            .unwrap_or_default())
    }
}

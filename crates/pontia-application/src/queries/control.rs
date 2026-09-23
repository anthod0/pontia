use super::ExternalQueryService;
use crate::SessionCapabilities;
use pontia_core::Result;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;

pub(crate) struct SessionControlState {
    pub session_id: String,
    pub client_type: String,
    pub state: String,
    pub capabilities: SessionCapabilities,
    pub workspace_id: Option<String>,
    pub workspace: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
}

impl ExternalQueryService {
    /// Reads only persisted control state and the shared capability policy.
    pub(crate) async fn get_session_control(
        &self,
        session_id: &str,
    ) -> Result<Option<SessionControlState>> {
        let Some(row) = SqliteSessionRepository::new(self.pool.clone())
            .get_session(session_id)
            .await?
        else {
            return Ok(None);
        };
        let capabilities = self
            .session_capabilities(session_id, &row.client_type)
            .await?;
        Ok(Some(SessionControlState {
            session_id: row.session_id,
            client_type: row.client_type,
            state: row.state,
            capabilities,
            workspace_id: row.workspace_id,
            workspace: row.workspace_ref,
            handle: row.handle,
            role: row.role,
        }))
    }
}

#[cfg(test)]
mod tests;

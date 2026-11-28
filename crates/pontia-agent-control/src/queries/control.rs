use super::ControlQueryService;
use crate::SessionCapabilities;
use pontia_core::Result;
use pontia_storage_sqlite::repositories::sessions::SqliteSessionRepository;

pub(crate) struct SessionControlState {
    pub session_id: String,
    pub client_type: String,
    pub state: String,
    pub supported_capabilities: SessionCapabilities,
    pub capabilities: SessionCapabilities,
    pub workspace_id: Option<String>,
    pub workspace: Option<String>,
    pub handle: Option<String>,
    pub role: Option<String>,
}

impl SessionControlState {
    pub(crate) fn require_capability(
        &self,
        enabled: fn(&SessionCapabilities) -> bool,
        operation: &str,
    ) -> Result<()> {
        if !enabled(&self.supported_capabilities) {
            return Err(pontia_core::Error::Unsupported(format!(
                "Session {} does not support {operation}",
                self.session_id
            )));
        }
        if !enabled(&self.capabilities) {
            return Err(pontia_core::Error::Unavailable(format!(
                "{operation} is unavailable for Session {}",
                self.session_id
            )));
        }
        Ok(())
    }
}

impl ControlQueryService {
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
        let runtimes = self.session_runtimes(session_id).await?;
        let capabilities = self
            .session_capabilities(session_id, &row.client_type, &runtimes)
            .await?;
        Ok(Some(SessionControlState {
            session_id: row.session_id,
            client_type: row.client_type,
            state: row.state,
            supported_capabilities: capabilities.supported,
            capabilities: capabilities.available,
            workspace_id: row.workspace_id,
            workspace: row.workspace_ref,
            handle: row.handle,
            role: row.role,
        }))
    }
}

#[cfg(test)]
mod tests;

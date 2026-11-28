use super::SessionCommandService;
use crate::runtime::ControlTarget;
use pontia_core::{Error, Result, ids::new_compaction_operation_id};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct CompactionAccepted {
    pub accepted: bool,
    pub operation_id: String,
}

impl SessionCommandService {
    pub async fn compact_session(&self, session_id: &str) -> Result<CompactionAccepted> {
        let actor = self.inbox.scheduler.session(session_id);
        let _guard = actor.lock().await;
        let session = self
            .queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if !matches!(session.state.as_str(), "idle" | "interrupted") {
            return Err(Error::StateConflict(
                "Compaction requires an idle or interrupted Session".into(),
            ));
        }
        session.require_capability(|caps| caps.manual_compaction, "manual compaction")?;
        let target =
            ControlTarget::resolve(&self.pool, self.clients.registry(), session_id, None).await?;
        let operation_id = new_compaction_operation_id().to_string();
        self.clients
            .for_client(&session.client_type)?
            .compact(&target, &operation_id)
            .await
            .into_result()?;
        Ok(CompactionAccepted {
            accepted: true,
            operation_id,
        })
    }
}

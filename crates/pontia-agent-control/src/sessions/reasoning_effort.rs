use super::SessionCommandService;
use crate::runtime::ControlTarget;
use pontia_core::{Error, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetSessionReasoningEffortRequest {
    pub reasoning_effort: String,
}

impl SessionCommandService {
    pub async fn set_reasoning_effort(
        &self,
        session_id: &str,
        request: SetSessionReasoningEffortRequest,
    ) -> Result<()> {
        if request.reasoning_effort.trim().is_empty() {
            return Err(Error::Domain("reasoning_effort must be non-empty".into()));
        }
        let session = self
            .queries
            .get_session_control(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if !session.capabilities.reasoning_effort_control {
            return Err(Error::CapabilityUnavailable(
                "Reasoning effort control is unavailable".into(),
            ));
        }
        if !matches!(
            session.state.as_str(),
            "idle" | "busy" | "waiting_for_interaction"
        ) {
            return Err(Error::StateConflict(
                "The Session must be running to change reasoning effort".into(),
            ));
        }
        let target =
            ControlTarget::resolve(&self.pool, self.clients.registry(), session_id, None).await?;
        self.clients
            .for_client(&session.client_type)?
            .set_reasoning_effort(&target, &request.reasoning_effort)
            .await
            .into_result()
    }
}

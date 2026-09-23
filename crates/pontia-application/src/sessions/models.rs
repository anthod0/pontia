use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::SessionCommandService;
use crate::{
    ExternalQueryService, SessionView, clients::ClientAdapter,
    runtime::control_target::ControlTarget,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionModel {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct SessionModels {
    pub models: Vec<SessionModel>,
    pub current_model: Option<String>,
    pub runtime_instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetSessionModelRequest {
    pub model: String,
    pub runtime_instance_id: String,
}

impl SessionCommandService {
    pub async fn list_session_models(&self, session_id: &str) -> Result<SessionModels> {
        let session = self.model_session(session_id, false).await?;
        let target = ControlTarget::resolve(&self.pool, session_id, None).await?;
        let runtime_instance_id = target.instance()?.to_owned();
        let models = ClientAdapter::new(
            &session.client_type,
            self.event_ingest.clone(),
            self.client_control.clone(),
        )?
        .list_models(&target)
        .await?;
        let current_model = ExternalQueryService::new(self.pool.clone())
            .with_clients(self.event_ingest.clients())
            .get_session(session_id)
            .await?
            .and_then(|session| session.model);
        Ok(SessionModels {
            models,
            current_model,
            runtime_instance_id,
        })
    }

    pub async fn set_session_model(
        &self,
        session_id: &str,
        request: SetSessionModelRequest,
    ) -> Result<()> {
        if request.model.trim().is_empty() || request.runtime_instance_id.trim().is_empty() {
            return Err(Error::Domain(
                "model and runtime_instance_id must be non-empty".into(),
            ));
        }
        let session = self.model_session(session_id, true).await?;
        let target =
            ControlTarget::resolve(&self.pool, session_id, Some(&request.runtime_instance_id))
                .await?;
        ClientAdapter::new(
            &session.client_type,
            self.event_ingest.clone(),
            self.client_control.clone(),
        )?
        .set_model(&target, &request.model)
        .await
        .into_result()
    }

    async fn model_session(&self, session_id: &str, modifying: bool) -> Result<SessionView> {
        let session = ExternalQueryService::new(self.pool.clone())
            .with_clients(self.event_ingest.clients())
            .get_session(session_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("session {session_id} not found")))?;
        if !session.capabilities.list_models || (modifying && !session.capabilities.set_model) {
            return Err(Error::CapabilityUnavailable(
                "This client does not support this model operation.".into(),
            ));
        }
        if !matches!(session.state.as_str(), "idle" | "busy") {
            return Err(Error::StateConflict(
                "The session must be running to choose a model.".into(),
            ));
        }
        if let Some(reason) = &session.model_control_unavailable_reason {
            return Err(Error::CapabilityUnavailable(reason.clone()));
        }
        Ok(session)
    }
}

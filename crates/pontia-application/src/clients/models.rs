use pontia_agent_clients::DispatchMode;
use pontia_core::{Error, Result};

use super::ClientAdapter;
use crate::{
    control::ControlResult, runtime::control_target::ControlTarget, sessions::SessionModel,
};

impl ClientAdapter {
    pub async fn list_models(&self, target: &ControlTarget) -> Result<Vec<SessionModel>> {
        if self.spec.adapter.dispatch != DispatchMode::CodexProtocol {
            return Err(Error::CapabilityUnavailable(
                "Client model listing is unsupported".into(),
            ));
        }
        crate::codex::CodexService::new(self.events.clone())
            .list_models(target)
            .await
    }

    pub async fn set_model(&self, target: &ControlTarget, model: &str) -> ControlResult<()> {
        if self.spec.adapter.dispatch != DispatchMode::CodexProtocol {
            return ControlResult::Unsupported("Client model selection is unsupported".into());
        }
        ControlResult::from_result(
            crate::codex::CodexService::new(self.events.clone())
                .set_model(target, model)
                .await,
        )
    }
}

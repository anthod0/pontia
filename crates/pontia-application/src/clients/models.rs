use pontia_agent_clients::DispatchMode;
use pontia_core::{Error, Result};

use super::ClientAdapter;
use crate::{
    control::ControlResult, runtime::control_target::ControlTarget, sessions::SessionModel,
};

impl ClientAdapter {
    pub async fn list_models(&self, target: &ControlTarget) -> Result<Vec<SessionModel>> {
        match self.spec.adapter.dispatch {
            DispatchMode::CodexProtocol => {
                crate::codex::CodexService::new(self.events.clone())
                    .list_models(target)
                    .await
            }
            DispatchMode::Connected => {
                self.channel_models()?
                    .list_models(&target.session_id, target.instance()?)
                    .await
            }
            _ => Err(Error::CapabilityUnavailable(
                "Client model listing is unsupported".into(),
            )),
        }
    }

    pub async fn set_model(&self, target: &ControlTarget, model: &str) -> ControlResult<()> {
        ControlResult::from_result(
            async {
                match self.spec.adapter.dispatch {
                    DispatchMode::CodexProtocol => {
                        crate::codex::CodexService::new(self.events.clone())
                            .set_model(target, model)
                            .await
                    }
                    DispatchMode::Connected => {
                        self.channel_models()?
                            .set_model(&target.session_id, target.instance()?, model)
                            .await
                    }
                    _ => Err(Error::CapabilityUnavailable(
                        "Client model selection is unsupported".into(),
                    )),
                }
            }
            .await,
        )
    }

    fn channel_models(&self) -> Result<&crate::ClientControlService> {
        self.control.as_ref().ok_or_else(|| {
            Error::CapabilityUnavailable("Client control service is unavailable".into())
        })
    }
}

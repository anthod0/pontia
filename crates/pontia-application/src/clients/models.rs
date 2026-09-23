use crate::client_contract::DispatchMode;
use pontia_core::{Error, Result};

use super::ClientAdapter;
use crate::{
    control::ControlResult, runtime::control_target::ControlTarget, sessions::SessionModel,
};

impl ClientAdapter {
    pub async fn list_models(&self, target: &ControlTarget) -> Result<Vec<SessionModel>> {
        if let Some(client) = self.session_client() {
            return client.list_models(self.events.clone(), target).await;
        }
        match self.spec.adapter.dispatch {
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
                if let Some(client) = self.session_client() {
                    return client.set_model(self.events.clone(), target, model).await;
                }
                match self.spec.adapter.dispatch {
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

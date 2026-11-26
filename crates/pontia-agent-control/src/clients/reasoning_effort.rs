use super::ClientAdapter;
use crate::{client_contract::DispatchMode, control::ControlResult, runtime::ControlTarget};
use pontia_core::Error;

impl ClientAdapter {
    pub async fn set_reasoning_effort(
        &self,
        target: &ControlTarget,
        reasoning_effort: &str,
    ) -> ControlResult<()> {
        ControlResult::from_result(
            async {
                target.validate(&self.pool).await?;
                if let Some(client) = self.session_client() {
                    return client
                        .set_reasoning_effort(self.events.clone(), target, reasoning_effort)
                        .await;
                }
                match self.spec.adapter.dispatch {
                    DispatchMode::Connected => {
                        self.control
                            .set_reasoning_effort(
                                &target.session_id,
                                target.instance()?,
                                reasoning_effort,
                            )
                            .await
                    }
                    _ => Err(Error::CapabilityUnavailable(
                        "Reasoning effort control is unsupported".into(),
                    )),
                }
            }
            .await,
        )
    }
}

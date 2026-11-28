use super::ClientAdapter;
use crate::{control::ControlResult, runtime::ControlTarget};
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
                Err(Error::Unsupported(
                    "Reasoning effort control is unsupported".into(),
                ))
            }
            .await,
        )
    }
}

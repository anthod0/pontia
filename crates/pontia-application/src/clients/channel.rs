use super::ClientAdapter;
use crate::{RuntimeReadinessService, runtime::ControlTarget};
use pontia_core::Result;

impl ClientAdapter {
    pub(super) async fn channel_input(
        &self,
        target: &ControlTarget,
        input: &str,
        message: Option<&str>,
    ) -> Result<()> {
        RuntimeReadinessService::new(self.pool.clone())
            .wait_until_ready(
                &target.session_id,
                self.spec.client_type,
                target.instance()?,
            )
            .await?;
        target.validate(&self.pool.clone()).await?;
        self.control
            .submit(&target.session_id, target.instance()?, input, message)
            .await
    }

    pub(super) async fn channel_interrupt(&self, target: &ControlTarget) -> Result<()> {
        target.validate(&self.pool.clone()).await?;
        self.control
            .interrupt(&target.session_id, target.instance()?)
            .await
    }

    pub async fn replay(
        &self,
        target: &ControlTarget,
        message: &str,
    ) -> crate::control::ControlResult<()> {
        if !self.spec.capabilities.branch_control {
            return crate::control::ControlResult::Unsupported(
                "client does not support branch control".into(),
            );
        }
        let result = async {
            RuntimeReadinessService::new(self.pool.clone())
                .wait_until_ready(
                    &target.session_id,
                    self.spec.client_type,
                    target.instance()?,
                )
                .await?;
            target.validate(&self.pool.clone()).await?;
            self.control
                .replay(&target.session_id, target.instance()?, message)
                .await
        }
        .await;
        match result {
            Ok(()) => crate::control::ControlResult::Sent(()),
            Err(error) => crate::control::ControlResult::from_result(Err(error)),
        }
    }
}

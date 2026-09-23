use super::ClientAdapter;
use crate::{RuntimeReadinessService, runtime::control_target::ControlTarget};
use pontia_core::{Error, Result};

impl ClientAdapter {
    pub(super) async fn channel_input(
        &self,
        target: &ControlTarget,
        input: &str,
        message: Option<&str>,
    ) -> Result<()> {
        RuntimeReadinessService::new(self.events.db())
            .wait_until_ready(
                &target.session_id,
                self.spec.client_type,
                target.instance()?,
            )
            .await?;
        target.validate(&self.events.db()).await?;
        self.control
            .as_ref()
            .ok_or_else(|| {
                Error::CapabilityUnavailable("Client control service is unavailable".into())
            })?
            .submit(&target.session_id, target.instance()?, input, message)
            .await
    }

    pub(super) async fn channel_interrupt(&self, target: &ControlTarget) -> Result<()> {
        target.validate(&self.events.db()).await?;
        self.control
            .as_ref()
            .ok_or_else(|| {
                Error::CapabilityUnavailable("Client control service is unavailable".into())
            })?
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
            RuntimeReadinessService::new(self.events.db())
                .wait_until_ready(
                    &target.session_id,
                    self.spec.client_type,
                    target.instance()?,
                )
                .await?;
            target.validate(&self.events.db()).await?;
            self.control
                .as_ref()
                .ok_or_else(|| {
                    Error::CapabilityUnavailable("Client control service is unavailable".into())
                })?
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

impl ClientAdapter {
    pub fn branch_target(
        &self,
        binding: pontia_storage_sqlite::models::agent_bindings::AgentBindingRow,
        target: pontia_storage_sqlite::models::turns::TurnProjectionRow,
        is_first_session_turn: bool,
    ) -> Result<String> {
        let clients = self.events.clients();
        let data = clients.data(self.spec.client_type).ok_or_else(|| {
            Error::CapabilityUnavailable("branch target source unavailable".into())
        })?;
        data.branch_target(super::BranchTargetRequest {
            binding,
            turn: target,
            is_first_session_turn,
        })
    }
}

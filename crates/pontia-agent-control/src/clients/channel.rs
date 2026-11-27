use super::ClientAdapter;
use crate::{
    RuntimeReadinessService,
    client_contract::{DeliveryError, InputPayload},
    runtime::ControlTarget,
};

impl ClientAdapter {
    pub(super) async fn channel_start_turn(
        &self,
        target: &ControlTarget,
        payload: &InputPayload,
        message: Option<&str>,
    ) -> Result<(), DeliveryError> {
        RuntimeReadinessService::new(self.pool.clone(), self.spec.adapter.control_runtime_role)
            .wait_until_ready(
                &target.session_id,
                self.spec.client_type,
                target.instance().map_err(DeliveryError::from)?,
            )
            .await
            .map_err(DeliveryError::from)?;
        target
            .validate(&self.pool.clone())
            .await
            .map_err(DeliveryError::from)?;
        self.control
            .start_turn(
                &target.session_id,
                target.instance().map_err(DeliveryError::from)?,
                payload,
                message,
            )
            .await
    }

    pub(super) async fn channel_steer(
        &self,
        target: &ControlTarget,
        payload: &InputPayload,
        message: Option<&str>,
    ) -> Result<(), DeliveryError> {
        RuntimeReadinessService::new(self.pool.clone(), self.spec.adapter.control_runtime_role)
            .wait_until_ready(
                &target.session_id,
                self.spec.client_type,
                target.instance().map_err(DeliveryError::from)?,
            )
            .await
            .map_err(DeliveryError::from)?;
        target
            .validate(&self.pool.clone())
            .await
            .map_err(DeliveryError::from)?;
        self.control
            .steer(
                &target.session_id,
                target.instance().map_err(DeliveryError::from)?,
                payload,
                message,
            )
            .await
    }

    pub(super) async fn channel_interrupt(
        &self,
        target: &ControlTarget,
    ) -> Result<(), DeliveryError> {
        target
            .validate(&self.pool.clone())
            .await
            .map_err(DeliveryError::from)?;
        self.control
            .interrupt(
                &target.session_id,
                target.instance().map_err(DeliveryError::from)?,
            )
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
            RuntimeReadinessService::new(self.pool.clone(), self.spec.adapter.control_runtime_role)
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

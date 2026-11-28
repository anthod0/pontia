use crate::client_contract::{DeliveryError, DispatchMode, InputPayload};
use pontia_core::Result;

use super::ClientAdapter;
use crate::{
    control::{ControlResult, InputReceipt},
    runtime::ControlTarget,
};

impl ClientAdapter {
    pub async fn await_initial_ready(&self, target: &ControlTarget) -> Result<()> {
        target.validate(&self.pool.clone()).await?;
        if !self.prepares_on_input() && self.spec.adapter.dispatch == DispatchMode::Connected {
            crate::RuntimeReadinessService::new(
                self.pool.clone(),
                self.spec.adapter.control_runtime_role,
            )
            .wait_until_ready(
                &target.session_id,
                self.spec.client_type,
                target.instance()?,
            )
            .await?;
        }
        target.validate(&self.pool.clone()).await
    }

    pub async fn start_turn(
        &self,
        target: &ControlTarget,
        payload: InputPayload,
        turn: Option<&str>,
    ) -> ControlResult<InputReceipt> {
        if self.spec.adapter.dispatch == DispatchMode::None {
            return ControlResult::Unsupported("client has no input channel".into());
        }
        let result = self.start_turn_inner(target, payload, turn).await;
        ControlResult::from_delivery(result)
    }

    async fn start_turn_inner(
        &self,
        target: &ControlTarget,
        payload: InputPayload,
        turn: Option<&str>,
    ) -> std::result::Result<InputReceipt, DeliveryError> {
        target
            .validate(&self.pool.clone())
            .await
            .map_err(DeliveryError::from)?;
        let client = self.session_client().ok_or_else(|| {
            DeliveryError::Offline("client Session control is unavailable".into())
        })?;
        let _launch = self
            .prepares_on_input()
            .then(|| pontia_runtime::launch_lock::launch(&self.pontia_home))
            .transpose()
            .map_err(DeliveryError::from)?;
        client
            .start_turn(self.events.clone(), target, &payload, turn)
            .await
    }

    pub async fn steer(
        &self,
        target: &ControlTarget,
        payload: InputPayload,
    ) -> ControlResult<InputReceipt> {
        if self.spec.adapter.dispatch == DispatchMode::None {
            return ControlResult::Unsupported("client has no input channel".into());
        }
        let result = async {
            target
                .validate(&self.pool.clone())
                .await
                .map_err(DeliveryError::from)?;
            self.session_client()
                .ok_or_else(|| {
                    DeliveryError::Offline("client Session control is unavailable".into())
                })?
                .steer(self.events.clone(), target, &payload)
                .await
        }
        .await;
        ControlResult::from_delivery(result)
    }

    pub async fn interrupt(&self, target: &ControlTarget) -> ControlResult<()> {
        if !self.spec.capabilities.interrupt {
            return ControlResult::Unsupported("client does not support interrupt".into());
        }
        let Some(client) = self.session_client() else {
            return ControlResult::from_result(Err(pontia_core::Error::CapabilityUnavailable(
                "client Session control is unavailable".into(),
            )));
        };
        ControlResult::from_delivery(client.interrupt(self.events.clone(), target).await)
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
        let prepared = async {
            crate::RuntimeReadinessService::new(
                self.pool.clone(),
                self.spec.adapter.control_runtime_role,
            )
            .wait_until_ready(
                &target.session_id,
                self.spec.client_type,
                target.instance()?,
            )
            .await?;
            target.validate(&self.pool).await?;
            Ok(())
        }
        .await;
        if let Err(error) = prepared {
            return crate::control::ControlResult::from_result(Err(error));
        }
        let Some(client) = self.session_client() else {
            return crate::control::ControlResult::from_result(Err(
                pontia_core::Error::CapabilityUnavailable(
                    "client Session control is unavailable".into(),
                ),
            ));
        };
        match client.replay(self.events.clone(), target, message).await {
            Ok(()) => crate::control::ControlResult::Sent(()),
            Err(error) => crate::control::ControlResult::from_delivery(Err(error)),
        }
    }
}

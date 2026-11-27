use crate::client_contract::{DeliveryError, DispatchMode, InputPayload};
use pontia_core::{Error, Result, ids::new_dispatch_id};
use pontia_runtime::AgentInput;

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
        if let Some(client) = self.session_client() {
            let _launch = self
                .prepares_on_input()
                .then(|| pontia_runtime::launch_lock::launch(&self.control.pontia_home))
                .transpose()
                .map_err(DeliveryError::from)?;
            return client
                .start_turn(self.events.clone(), target, &payload, turn)
                .await;
        }
        let input = AgentInput {
            session_id: target.session_id.clone(),
            dispatch_id: new_dispatch_id().to_string(),
            input: payload.text.clone(),
        };
        match self.spec.adapter.dispatch {
            DispatchMode::Connected => self.channel_start_turn(target, &payload).await?,
            DispatchMode::InProcessRecorded => {
                self.registry
                    .get(self.spec.client_type)
                    .and_then(|entry| entry.in_process.as_ref())
                    .ok_or_else(|| {
                        DeliveryError::Offline("in-process client is unavailable".into())
                    })?
                    .submit(input)
                    .map_err(|error| match error {
                        Error::Domain(message) => DeliveryError::StateChanged(message),
                        error => DeliveryError::from(error),
                    })?;
            }
            DispatchMode::None => {
                return Err(DeliveryError::Unsupported(
                    "client has no input channel".into(),
                ));
            }
        }
        Ok(InputReceipt {
            turn_pending: false,
            native_turn_id: None,
            runtime_id: target.runtime_id.clone(),
        })
    }

    pub async fn steer(
        &self,
        target: &ControlTarget,
        payload: InputPayload,
    ) -> ControlResult<InputReceipt> {
        if self.spec.adapter.dispatch == DispatchMode::None {
            return ControlResult::Unsupported("client has no input channel".into());
        }
        if self.spec.adapter.dispatch == DispatchMode::InProcessRecorded {
            let result = async {
                target.validate(&self.pool.clone()).await?;
                self.registry
                    .get(self.spec.client_type)
                    .and_then(|entry| entry.in_process.as_ref())
                    .ok_or_else(|| {
                        pontia_core::Error::CapabilityUnavailable(
                            "in-process client is unavailable".into(),
                        )
                    })?
                    .submit(AgentInput {
                        session_id: target.session_id.clone(),
                        dispatch_id: new_dispatch_id().to_string(),
                        input: payload.text.clone(),
                    })?;
                Ok(InputReceipt {
                    turn_pending: false,
                    native_turn_id: None,
                    runtime_id: target.runtime_id.clone(),
                })
            }
            .await;
            return ControlResult::from_result(result);
        }
        let result = async {
            target
                .validate(&self.pool.clone())
                .await
                .map_err(DeliveryError::from)?;
            if let Some(client) = self.session_client() {
                return client.steer(self.events.clone(), target, &payload).await;
            }
            match self.spec.adapter.dispatch {
                DispatchMode::Connected => {
                    self.channel_steer(target, &payload).await?;
                }
                DispatchMode::InProcessRecorded | DispatchMode::None => {
                    return Err(DeliveryError::Unsupported(
                        "client does not support steer".into(),
                    ));
                }
            }
            Ok(InputReceipt {
                turn_pending: false,
                native_turn_id: None,
                runtime_id: target.runtime_id.clone(),
            })
        }
        .await;
        ControlResult::from_delivery(result)
    }

    pub async fn interrupt(&self, target: &ControlTarget) -> ControlResult<()> {
        if !self.spec.capabilities.interrupt {
            return ControlResult::Unsupported("client does not support interrupt".into());
        }
        if let Some(client) = self.session_client() {
            return ControlResult::from_delivery(
                client.interrupt(self.events.clone(), target).await,
            );
        }
        ControlResult::from_delivery(self.channel_interrupt(target).await)
    }
}

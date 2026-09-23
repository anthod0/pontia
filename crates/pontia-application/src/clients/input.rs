use pontia_agent_clients::DispatchMode;
use pontia_core::{Error, Result, ids::new_dispatch_id};
use pontia_runtime::{AgentInput, GenericRuntimeManager};
use serde_json::Value;

use super::ClientAdapter;
use crate::{
    control::{ControlResult, InputReceipt},
    runtime::control_target::ControlTarget,
    turns::InputIntent,
};

impl ClientAdapter {
    pub async fn await_initial_ready(&self, target: &ControlTarget) -> Result<()> {
        target.validate(&self.events.db()).await?;
        if self.spec.adapter.dispatch == DispatchMode::PiControl {
            crate::RuntimeReadinessService::new(self.events.db())
                .wait_until_ready(
                    &target.session_id,
                    self.spec.client_type,
                    target.instance()?,
                )
                .await?;
        }
        target.validate(&self.events.db()).await
    }

    pub async fn input(
        &self,
        target: &ControlTarget,
        input: String,
        metadata: &Value,
        intent: &InputIntent,
    ) -> ControlResult<InputReceipt> {
        if self.spec.adapter.dispatch == DispatchMode::None {
            return ControlResult::Unsupported("client has no input channel".into());
        }
        if matches!(intent, InputIntent::Steer { .. }) && !self.supports_steer() {
            return ControlResult::Unsupported("This client does not support steer".into());
        }
        let result = self.input_inner(target, input, metadata, intent).await;
        ControlResult::from_result(result)
    }

    async fn input_inner(
        &self,
        target: &ControlTarget,
        input: String,
        metadata: &Value,
        intent: &InputIntent,
    ) -> Result<InputReceipt> {
        target.validate(&self.events.db()).await?;
        if self.spec.adapter.dispatch == DispatchMode::CodexProtocol {
            return crate::codex::CodexService::new(self.events.clone())
                .submit(
                    target,
                    &input,
                    metadata["inbox_message_id"].as_str(),
                    intent,
                )
                .await;
        }
        let input = AgentInput {
            session_id: target.session_id.clone(),
            dispatch_id: new_dispatch_id().to_string(),
            input,
        };
        match self.spec.adapter.dispatch {
            DispatchMode::PiControl => {
                self.pi_input(target, &input.input, metadata["inbox_message_id"].as_str())
                    .await?
            }
            DispatchMode::InProcessRecorded => {
                GenericRuntimeManager.submit_input(self.spec.client_type, input)?;
            }
            DispatchMode::None => {
                return Err(Error::CapabilityUnavailable(
                    "client has no input channel".into(),
                ));
            }
            DispatchMode::CodexProtocol => unreachable!(),
        }
        Ok(InputReceipt {
            native_turn_id: None,
            runtime_instance_id: target.runtime_instance_id.clone(),
        })
    }

    pub async fn interrupt(&self, target: &ControlTarget, turn: &str) -> ControlResult<()> {
        if !self.spec.capabilities.interrupt {
            return ControlResult::Unsupported("client does not support interrupt".into());
        }
        if self.spec.adapter.dispatch == DispatchMode::CodexProtocol {
            return ControlResult::from_result(
                crate::codex::CodexService::new(self.events.clone())
                    .interrupt(target, turn)
                    .await,
            );
        }
        ControlResult::from_result(self.pi_interrupt(target).await)
    }
}

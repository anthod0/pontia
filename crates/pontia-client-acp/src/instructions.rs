use pontia_agent_control::client_contract::{ClientInstructions, InstructionsBindingInput};
use pontia_core::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) struct InstructionsPolicy;

impl ClientInstructions for InstructionsPolicy {
    fn inject_standalone_session_instructions(&self) -> bool {
        true
    }

    fn bind(&self, input: &InstructionsBindingInput<'_>) -> Result<Value> {
        Ok(serde_json::to_value(Instructions {
            system_prompt: input.system_prompt.map(str::to_owned),
            required: input.instructions_required,
        })?)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Instructions {
    pub system_prompt: Option<String>,
    pub required: bool,
}

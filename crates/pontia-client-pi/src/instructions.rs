use pontia_agent_control::client_contract::{ClientInstructions, InstructionsBindingInput};
use pontia_core::Result;
use serde_json::{Value, json};

pub(crate) struct InstructionsPolicy;
impl ClientInstructions for InstructionsPolicy {
    fn inject_standalone_session_instructions(&self) -> bool {
        true
    }
    fn bind(&self, input: &InstructionsBindingInput<'_>) -> Result<Value> {
        Ok(json!({"system_prompt": input.system_prompt}))
    }
}

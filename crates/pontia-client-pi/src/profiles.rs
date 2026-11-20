use pontia_agent_control::client_contract::{ClientProfile, ProfileBindingInput};
use pontia_core::Result;
use serde_json::{Value, json};

pub(crate) struct ProfilePolicy;
impl ClientProfile for ProfilePolicy {
    fn inject_standalone_session_instructions(&self) -> bool {
        true
    }
    fn validate_templates(&self, _: Option<&str>, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn bind(&self, input: &ProfileBindingInput<'_>) -> Result<Value> {
        Ok(json!({"system_prompt": input.system_prompt}))
    }
}

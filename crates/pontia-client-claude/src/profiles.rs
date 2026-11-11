use pontia_agent_control::{ExecutionProfileView, client_contract::ClientProfile};
use pontia_core::Result;
use serde_json::{Value, json};

pub(crate) struct ProfilePolicy;
impl ClientProfile for ProfilePolicy {
    fn inject_builtin(&self) -> bool {
        true
    }
    fn supports_explicit(&self) -> bool {
        false
    }
    fn validate_templates(&self, _: Option<&str>, _: Option<&str>) -> Result<()> {
        Ok(())
    }
    fn bind(&self, profile: &ExecutionProfileView) -> Result<Value> {
        Ok(json!({"system_prompt": profile.system_prompt_template}))
    }
}

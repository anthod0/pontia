use pontia_agent_control::{ExecutionProfileView, client_contract::ClientProfile};
use pontia_core::{Error, Result};
use serde_json::{Value, json};

pub(crate) struct ProfilePolicy;
impl ClientProfile for ProfilePolicy {
    fn inject_builtin(&self) -> bool {
        true
    }
    fn validate_templates(&self, system: Option<&str>, turn: Option<&str>) -> Result<()> {
        if turn.is_some() {
            return Err(Error::Domain(
                "Claude Profiles use system_prompt_template; omit turn_prompt_template".into(),
            ));
        }
        if let Some(system) = system {
            if system.trim().is_empty() {
                return Err(Error::Domain(
                    "system_prompt_template cannot be empty".into(),
                ));
            }
            if system.contains("{{") || system.contains("}}") {
                return Err(Error::Domain(
                    "Claude system_prompt_template supports static instructions only".into(),
                ));
            }
        }
        Ok(())
    }
    fn bind(&self, profile: &ExecutionProfileView, _instructions_required: bool) -> Result<Value> {
        Ok(json!({"system_prompt": profile.system_prompt_template}))
    }
}

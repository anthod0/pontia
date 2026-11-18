use pontia_agent_control::{ExecutionProfileView, client_contract::ClientProfile};
use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) struct ProfilePolicy;

impl ClientProfile for ProfilePolicy {
    fn inject_builtin(&self) -> bool {
        true
    }

    fn validate_templates(&self, system: Option<&str>, turn: Option<&str>) -> Result<()> {
        if turn.is_some() {
            return Err(Error::Domain(
                "ACP Profiles use system_prompt_template; omit turn_prompt_template".into(),
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
                    "ACP system_prompt_template supports static instructions only".into(),
                ));
            }
        }
        Ok(())
    }

    fn bind(&self, profile: &ExecutionProfileView, instructions_required: bool) -> Result<Value> {
        Ok(serde_json::to_value(Instructions {
            system_prompt: profile.system_prompt_template.clone(),
            required: instructions_required,
        })?)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Instructions {
    pub system_prompt: Option<String>,
    pub required: bool,
}

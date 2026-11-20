use pontia_core::Result;
use serde_json::Value;

pub struct InstructionsBindingInput<'a> {
    pub system_prompt: Option<&'a str>,
    /// Distinguishes caller-requested instructions from standalone Session instructions.
    pub instructions_required: bool,
}

/// Interprets client-specific instruction constraints and creation-time content.
pub trait ClientInstructions: Send + Sync {
    fn inject_standalone_session_instructions(&self) -> bool;
    fn validate_instructions(&self, system: Option<&str>) -> Result<()> {
        crate::sessions::instructions::validate_static_instructions(system)
    }
    fn bind(&self, input: &InstructionsBindingInput<'_>) -> Result<Value>;
}

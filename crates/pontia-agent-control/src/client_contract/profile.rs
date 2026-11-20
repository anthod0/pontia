use pontia_core::Result;
use serde_json::Value;

pub struct ProfileBindingInput<'a> {
    pub profile_id: Option<&'a str>,
    pub version: Option<&'a str>,
    pub system_prompt: Option<&'a str>,
    /// Distinguishes caller-requested instructions from standalone Session instructions.
    pub instructions_required: bool,
}

/// Interprets client-specific instruction constraints and creation-time content.
pub trait ClientProfile: Send + Sync {
    fn inject_standalone_session_instructions(&self) -> bool {
        false
    }
    fn validate_templates(&self, system: Option<&str>, turn: Option<&str>) -> Result<()>;
    fn bind(&self, input: &ProfileBindingInput<'_>) -> Result<Value>;
}

pub struct ResolvedClientProfile {
    pub version: Option<String>,
    pub binding: Value,
}

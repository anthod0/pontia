use crate::ExecutionProfileView;
use pontia_core::Result;
use serde_json::Value;

/// Interprets client-specific instruction constraints and creation-time content.
pub trait ClientProfile: Send + Sync {
    fn inject_builtin(&self) -> bool {
        false
    }
    fn validate_templates(&self, system: Option<&str>, turn: Option<&str>) -> Result<()>;
    /// `instructions_required` distinguishes caller-requested instructions from builtin defaults.
    fn bind(&self, profile: &ExecutionProfileView, instructions_required: bool) -> Result<Value>;
}

pub struct ResolvedClientProfile {
    pub version: Option<String>,
    pub binding: Value,
}

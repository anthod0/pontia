use crate::ExecutionProfileView;
use pontia_core::Result;
use serde_json::Value;

/// Interprets client-specific instruction constraints and creation-time content.
pub trait ClientProfile: Send + Sync {
    fn inject_builtin(&self) -> bool {
        false
    }
    fn supports_explicit(&self) -> bool {
        true
    }
    fn validate_templates(&self, system: Option<&str>, turn: Option<&str>) -> Result<()>;
    fn bind(&self, profile: &ExecutionProfileView) -> Result<Value>;
}

pub struct ResolvedClientProfile {
    pub version: Option<String>,
    pub binding: Value,
}

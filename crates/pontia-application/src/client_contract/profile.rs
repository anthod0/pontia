use crate::ExecutionProfileView;
use pontia_core::Result;
use serde_json::Value;

/// Interprets client-specific instruction constraints and creation-time content.
pub trait ClientProfile: Send + Sync {
    fn validate_templates(&self, system: Option<&str>, turn: Option<&str>) -> Result<()>;
    fn bind(&self, profile: &ExecutionProfileView) -> Result<Value>;
}

pub struct ResolvedClientProfile {
    pub version: String,
    pub binding: Value,
}

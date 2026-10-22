use pontia_core::Result;
use serde_json::Value;

/// Client-owned interpretation of management identities and effective configuration.
pub trait ClientConfiguration: Send + Sync {
    fn key(&self, metadata: &Value) -> Result<String>;
    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value>;
}

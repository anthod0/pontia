use pontia_core::Result;
use serde_json::Value;

/// Client-owned interpretation of management identities and effective configuration.
pub trait ClientConfiguration: Send + Sync {
    /// Identities supplied by defaults and the startup configuration file.
    fn keys(&self) -> Vec<String>;
    /// Display name and stable CLI selector, owned by the integration.
    fn presentation(&self, key: &str, effective: &Value) -> (String, String);
    fn key(&self, metadata: &Value) -> Result<String>;
    /// Merged values remain inspectable even when changed lower layers make them
    /// invalid for launching. Writes and Session creation must use `resolve`.
    fn merged(&self, key: &str, overrides: &Value) -> Value;
    fn resolve(&self, key: &str, overrides: &Value) -> Result<Value>;
}

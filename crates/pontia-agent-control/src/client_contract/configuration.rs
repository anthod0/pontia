use pontia_core::Result;
use serde_json::Value;

/// Client-owned interpretation of management identities and effective configuration.
pub trait ClientConfiguration: Send + Sync {
    fn list_models<'a>(
        &'a self,
        _effective: &'a Value,
        _directory: &'a std::path::Path,
    ) -> super::ClientOperation<'a, Vec<crate::sessions::SessionModel>> {
        Box::pin(async {
            Err(pontia_core::Error::CapabilityUnavailable(
                "This client does not support model listing before Session creation".into(),
            ))
        })
    }

    fn detect(
        &self,
        _effective: &Value,
        _context: &super::DetectionContext<'_>,
    ) -> super::ClientDetection {
        super::ClientDetection::unknown("No installation detector is available")
    }
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

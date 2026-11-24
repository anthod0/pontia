use pontia_core::Result;
use serde_json::Value;

/// Client-owned interpretation of management identities and effective configuration.
pub trait ClientConfiguration: Send + Sync {
    /// Apply any directory trust needed by this client beyond its normal launch behavior.
    fn trust_directory<'a>(
        &'a self,
        _key: &'a str,
        _directory: &'a std::path::Path,
    ) -> super::ClientOperation<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn supports_mode(&self, _key: &str, mode: pontia_core::SessionMode) -> bool {
        mode == pontia_core::SessionMode::Default
    }

    /// Client files applied when preparing a persistent private launch directory.
    fn private_directory_files(&self) -> &'static [super::LaunchFile] {
        &[]
    }

    fn supports_startup_reasoning_effort(&self) -> bool {
        false
    }

    fn supports_startup_model(&self) -> bool {
        false
    }

    fn list_models<'a>(
        &'a self,
        _effective: &'a Value,
        _directory: &'a std::path::Path,
    ) -> super::ClientOperation<'a, crate::agent_clients::AgentClientModels> {
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
    /// Merged values remain inspectable even when the startup configuration makes them
    /// invalid for launching. Writes and Session creation must use `resolve`.
    fn merged(&self, key: &str) -> Value;
    fn resolve(&self, key: &str) -> Result<Value>;
}

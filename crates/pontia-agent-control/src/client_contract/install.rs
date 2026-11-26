//! Agent Client installation capabilities registered by client integrations.

use super::DetectionContext;
use pontia_runtime::client_operations::InstallOperation;
use serde::{Deserialize, Serialize};
use std::io;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallProvenance {
    NotInstalled,
    PontiaManaged,
    VendorNative,
    NpmGlobal,
    Homebrew,
    ExternalUnmanaged,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrerequisiteStatus {
    pub id: String,
    pub label: String,
    pub required: bool,
    pub satisfied: bool,
    pub detected_detail: Option<String>,
    pub missing_hint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallMethod {
    pub id: String,
    pub label: String,
    pub recommended: bool,
    pub supported_on_platform: bool,
    pub source_summary: String,
    pub target_summary: String,
    pub prerequisites: Vec<PrerequisiteStatus>,
}

impl InstallMethod {
    pub fn ready(&self) -> bool {
        self.supported_on_platform
            && self
                .prerequisites
                .iter()
                .all(|p| !p.required || p.satisfied)
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Configured,
    NotConfigured,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthGuidance {
    pub status: AuthStatus,
    pub login_command: Option<String>,
    pub headless_login_command: Option<String>,
    pub env_vars: Vec<String>,
    pub docs_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallSpec {
    pub provenance: InstallProvenance,
    pub methods: Vec<InstallMethod>,
    pub auth: AuthGuidance,
}

pub trait ClientInstaller: Send + Sync {
    fn client_type(&self) -> &'static str;
    fn client_key(&self) -> &'static str;
    /// Inspect the device locally without modifying it or accessing the network.
    fn inspect(&self, context: &DetectionContext<'_>) -> InstallSpec;
    fn install(&self, method_id: &str, operation: &mut InstallOperation) -> io::Result<()>;
}

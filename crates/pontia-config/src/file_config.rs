use std::path::Path;

use pontia_core::error::{Error, Result};
use serde::Deserialize;

use super::{DashboardConfig, FilePickerConfig, RemoteConfig, WorkspaceRootConfig};

#[derive(Debug, Default, Deserialize)]
pub(super) struct FileConfig {
    pub(super) bind_addr: Option<String>,
    pub(super) external_api_token: Option<String>,
    pub(super) run_migrations: Option<bool>,
    pub(super) default_client_type: Option<String>,
    pub(super) runtime: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    pub(super) acp: Option<super::client_configuration::AcpFileConfig>,
    pub(super) workspace_browser: Option<FileWorkspaceBrowserConfig>,
    pub(super) file_picker: Option<FilePickerConfig>,
    pub(super) dashboard: Option<DashboardConfig>,
    pub(super) remote: Option<RemoteConfig>,
}

#[derive(Debug, Deserialize)]
pub(super) struct FileWorkspaceBrowserConfig {
    pub(super) roots: Option<Vec<WorkspaceRootConfig>>,
}

pub(super) fn read(path: &Path) -> Result<FileConfig> {
    let contents = std::fs::read_to_string(path).map_err(|err| Error::InvalidConfig {
        key: "PONTIA_HOME",
        message: format!("failed to read {}: {err}", path.display()),
    })?;
    toml::from_str(&contents).map_err(|err| Error::InvalidConfig {
        key: "PONTIA_HOME",
        message: format!("failed to parse {}: {err}", path.display()),
    })
}

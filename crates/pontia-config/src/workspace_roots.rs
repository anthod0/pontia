use pontia_core::error::{Error, Result};

use super::WorkspaceRootConfig;

pub(super) fn parse(value: &str) -> Result<Vec<WorkspaceRootConfig>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    trimmed
        .split(';')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let parts = entry.split('|').collect::<Vec<_>>();
            if parts.len() != 3 {
                return Err(Error::InvalidConfig {
                    key: "PONTIA_WORKSPACE_ROOTS",
                    message:
                        "expected entries formatted as root_id|label|path separated by semicolons"
                            .to_string(),
                });
            }
            let root_id = parts[0].trim();
            let label = parts[1].trim();
            let path = parts[2].trim();
            if root_id.is_empty() || label.is_empty() || path.is_empty() {
                return Err(Error::InvalidConfig {
                    key: "PONTIA_WORKSPACE_ROOTS",
                    message: "root_id, label, and path must be non-empty".to_string(),
                });
            }
            Ok(WorkspaceRootConfig {
                root_id: root_id.to_string(),
                label: label.to_string(),
                path: path.to_string(),
            })
        })
        .collect()
}

use std::path::{Path, PathBuf};

use pontia_config::WorkspaceRootConfig;
use pontia_core::error::{Error, Result};

pub(super) fn canonical_root(root: &WorkspaceRootConfig) -> Result<PathBuf> {
    let path = std::fs::canonicalize(&root.path)?;
    if !path.is_dir() {
        return Err(Error::NotFound(format!(
            "workspace root {} is not available",
            root.root_id
        )));
    }
    Ok(path)
}

pub(super) fn resolve_relative_path(root: &Path, relative_path: &str) -> Result<PathBuf> {
    let relative = Path::new(relative_path.trim());
    if relative.is_absolute() {
        return Err(Error::Domain(
            "workspace browser path must be relative".to_string(),
        ));
    }
    for component in relative.components() {
        match component {
            std::path::Component::Normal(_) | std::path::Component::CurDir => {}
            _ => {
                return Err(Error::Domain(
                    "workspace browser path cannot escape the configured root".to_string(),
                ));
            }
        }
    }
    let candidate = if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        root.join(relative)
    };
    let canonical = std::fs::canonicalize(&candidate).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            Error::NotFound(format!("directory {relative_path:?} not found"))
        } else {
            Error::Io(err)
        }
    })?;
    if !canonical.starts_with(root) {
        return Err(Error::Domain(
            "workspace browser path cannot escape the configured root".to_string(),
        ));
    }
    Ok(canonical)
}

pub(super) fn path_to_api_relative(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn should_skip_directory(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "target")
}

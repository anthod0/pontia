use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

use pontia_core::error::{Error, Result};

use super::environment::get;

pub(super) fn pontia_home(vars: &HashMap<String, String>) -> Result<PathBuf> {
    let value = get(vars, "PONTIA_HOME").ok_or_else(|| Error::InvalidConfig {
        key: "PONTIA_HOME",
        message: "must be set to a non-empty absolute path".to_string(),
    })?;
    if value.trim().is_empty() {
        return Err(Error::InvalidConfig {
            key: "PONTIA_HOME",
            message: "must be set to a non-empty absolute path".to_string(),
        });
    }

    let path = PathBuf::from(value);
    if !path.is_absolute()
        || path.parent().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::InvalidConfig {
            key: "PONTIA_HOME",
            message: "must be an absolute path without parent traversal".to_string(),
        });
    }
    Ok(path)
}

pub(super) fn config_path(home: &Path) -> PathBuf {
    home.join("config.toml")
}

pub(super) fn default_database_url(home: &Path) -> String {
    format!("sqlite://{}", home.join("data/pontia-e1.db").display())
}

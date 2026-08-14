use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

use pontia_core::error::{Error, Result};

use super::environment::get;

pub(super) fn pontia_home(vars: &HashMap<String, String>) -> Result<PathBuf> {
    if let Some(value) = get(vars, "PONTIA_HOME") {
        return validate_root("PONTIA_HOME", value);
    }

    let home = get(vars, "HOME").ok_or_else(|| Error::InvalidConfig {
        key: "PONTIA_HOME",
        message: "must be set when HOME is unavailable".to_string(),
    })?;
    Ok(validate_root("HOME", home)?.join(".pontia"))
}

fn validate_root(key: &'static str, value: &str) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if value.trim().is_empty()
        || !path.is_absolute()
        || path.parent().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::InvalidConfig {
            key,
            message: "must be a non-root absolute path without parent traversal".to_string(),
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

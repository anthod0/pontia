use std::{collections::HashMap, env, path::PathBuf};

use super::environment::get;

const DEFAULT_PONTIA_HOME: &str = "~/.pontia";

pub(super) fn pontia_home_dir() -> PathBuf {
    let vars: HashMap<String, String> = env::vars().collect();
    pontia_home_path(&vars)
}

pub(super) fn default_config_path_if_exists(vars: &HashMap<String, String>) -> Option<PathBuf> {
    let path = pontia_home_path(vars).join("config.toml");
    path.exists().then_some(path)
}

pub(super) fn default_database_url(vars: &HashMap<String, String>) -> String {
    format!(
        "sqlite://{}",
        pontia_home_string(vars).trim_end_matches('/').to_string() + "/data/pontia-e1.db"
    )
}

fn pontia_home_string(vars: &HashMap<String, String>) -> String {
    get(vars, "PONTIA_HOME")
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| DEFAULT_PONTIA_HOME.to_string())
}

fn pontia_home_path(vars: &HashMap<String, String>) -> PathBuf {
    expand_home_path(&pontia_home_string(vars))
}

fn expand_home_path(path: &str) -> PathBuf {
    if path == "~" {
        return env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

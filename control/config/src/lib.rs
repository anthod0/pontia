use std::{collections::HashMap, net::SocketAddr, path::PathBuf};

use serde::Deserialize;

mod environment;
mod file_config;
mod loading;
mod paths;
mod workspace_roots;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub external_api_token: Option<String>,
    pub run_migrations: bool,
    pub default_client_type: String,
    pub workspace_browser: WorkspaceBrowserConfig,
    pub file_picker: FilePickerConfig,
    pub runtime: RuntimeConfig,
    pub dashboard: DashboardConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct DashboardConfig {
    pub source: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default, flatten)]
    pub clients: HashMap<String, RuntimeClientConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RuntimeClientConfig {
    pub tui_command: Option<String>,
}

impl RuntimeConfig {
    pub fn tui_command_for_client_config_key(&self, runtime_config_key: &str) -> Option<String> {
        self.clients
            .get(runtime_config_key)
            .and_then(|client| client.tui_command.clone())
    }

    fn set_tui_command_for_client_config_key(
        &mut self,
        runtime_config_key: &str,
        command: Option<String>,
    ) {
        match command {
            Some(command) => {
                self.clients
                    .entry(runtime_config_key.to_string())
                    .or_default()
                    .tui_command = Some(command);
            }
            None => {
                if let Some(client) = self.clients.get_mut(runtime_config_key) {
                    client.tui_command = None;
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FilePickerConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub min_query_chars: usize,
    #[serde(default = "default_file_picker_max_results")]
    pub max_results: usize,
    #[serde(default = "default_file_picker_max_candidates")]
    pub max_candidates: usize,
    #[serde(default = "default_file_picker_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    #[serde(default = "default_true")]
    pub respect_ignore_files: bool,
    #[serde(default = "default_true")]
    pub respect_git_exclude: bool,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default)]
    pub follow_symlinks: bool,
    #[serde(default = "default_file_picker_ignore_globs")]
    pub ignore_globs: Vec<String>,
}

impl Default for FilePickerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_query_chars: 0,
            max_results: default_file_picker_max_results(),
            max_candidates: default_file_picker_max_candidates(),
            timeout_ms: default_file_picker_timeout_ms(),
            respect_gitignore: true,
            respect_ignore_files: true,
            respect_git_exclude: true,
            include_hidden: false,
            follow_symlinks: false,
            ignore_globs: default_file_picker_ignore_globs(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct WorkspaceBrowserConfig {
    pub roots: Vec<WorkspaceRootConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorkspaceRootConfig {
    pub root_id: String,
    pub label: String,
    pub path: String,
}

pub fn pontia_home_dir() -> PathBuf {
    paths::pontia_home_dir()
}

fn default_true() -> bool {
    true
}

fn default_file_picker_max_results() -> usize {
    100
}

fn default_file_picker_max_candidates() -> usize {
    100_000
}

fn default_file_picker_timeout_ms() -> u64 {
    1_500
}

fn default_file_picker_ignore_globs() -> Vec<String> {
    [
        ".git/**",
        "node_modules/**",
        "target/**",
        "dist/**",
        "build/**",
        ".svelte-kit/**",
        ".next/**",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

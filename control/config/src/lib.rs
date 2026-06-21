use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use pontia_agent_clients as agent_clients;
use pontia_core::error::{Error, Result};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_DATABASE_URL: &str = "sqlite://~/.local/share/pontia/pontia.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub external_api_token: Option<String>,
    pub run_migrations: bool,
    pub default_client_type: String,
    pub graph: GraphRuntimeConfig,
    pub workspace_browser: WorkspaceBrowserConfig,
    pub file_picker: FilePickerConfig,
    pub runtime: RuntimeConfig,
    pub dashboard: DashboardConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct DashboardConfig {
    pub source: Option<String>,
    pub cache_dir: Option<String>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphRuntimeConfig {
    pub enabled: bool,
    pub db_dir: Option<String>,
}

fn apply_runtime_env_overrides(vars: &HashMap<String, String>, runtime: &mut RuntimeConfig) {
    for client in agent_clients::AGENT_CLIENTS {
        let Some(tmux_runtime) = client.tmux_runtime() else {
            continue;
        };
        let (Some(env_key), Some(runtime_config_key)) =
            (tmux_runtime.command_env, tmux_runtime.runtime_config_key)
        else {
            continue;
        };
        if let Some(value) = get(vars, env_key) {
            runtime.set_tui_command_for_client_config_key(runtime_config_key, non_empty(value));
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    bind_addr: Option<String>,
    database_url: Option<String>,
    external_api_token: Option<String>,
    run_migrations: Option<bool>,
    default_client_type: Option<String>,
    runtime: Option<RuntimeConfig>,
    workspace_browser: Option<WorkspaceBrowserConfig>,
    file_picker: Option<FilePickerConfig>,
    dashboard: Option<DashboardConfig>,
}

pub fn config_path_from_args<I>(args: I) -> Result<Option<PathBuf>>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().skip(1);
    let mut config_path = None;
    while let Some(arg) = args.next() {
        if arg == "--config" {
            let path = args.next().ok_or_else(|| Error::InvalidConfig {
                key: "--config",
                message: "--config requires a path".to_string(),
            })?;
            config_path = Some(PathBuf::from(path));
        }
    }
    Ok(config_path)
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        let vars: HashMap<String, String> = env::vars().collect();
        let config_path = explicit_config_path(&vars).or_else(default_config_path_if_exists);
        Self::from_vars_and_file(&vars, config_path.as_deref())
    }

    pub fn from_env_with_config_path(config_path: Option<&Path>) -> Result<Self> {
        let _ = dotenvy::dotenv();
        let vars: HashMap<String, String> = env::vars().collect();
        let config_path = config_path
            .map(Path::to_path_buf)
            .or_else(|| explicit_config_path(&vars).or_else(default_config_path_if_exists));
        Self::from_vars_and_file(&vars, config_path.as_deref())
    }

    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self> {
        Self::from_vars_and_file(vars, None)
    }

    pub fn from_vars_and_file(
        vars: &HashMap<String, String>,
        config_path: Option<&Path>,
    ) -> Result<Self> {
        let file = match config_path {
            Some(path) => Some(read_file_config(path)?),
            None => None,
        };
        let file = file.as_ref();

        let bind_addr = get(vars, "PONTIA_BIND_ADDR")
            .or_else(|| file.and_then(|config| config.bind_addr.as_deref()))
            .unwrap_or(DEFAULT_BIND_ADDR)
            .parse::<SocketAddr>()
            .map_err(|err| Error::InvalidConfig {
                key: "PONTIA_BIND_ADDR",
                message: err.to_string(),
            })?;

        let database_url = get(vars, "PONTIA_DATABASE_URL")
            .or_else(|| file.and_then(|config| config.database_url.as_deref()))
            .unwrap_or(DEFAULT_DATABASE_URL)
            .to_string();

        let external_api_token = get(vars, "PONTIA_EXTERNAL_API_TOKEN")
            .or_else(|| file.and_then(|config| config.external_api_token.as_deref()))
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string);

        let run_migrations = match get(vars, "PONTIA_RUN_MIGRATIONS") {
            Some(value) => parse_bool("PONTIA_RUN_MIGRATIONS", value)?,
            None => file
                .and_then(|config| config.run_migrations)
                .unwrap_or(true),
        };

        let default_client_type = get(vars, "PONTIA_DEFAULT_CLIENT_TYPE")
            .or_else(|| file.and_then(|config| config.default_client_type.as_deref()))
            .unwrap_or(agent_clients::default_real_client_type())
            .to_string();
        validate_real_default_client_type("PONTIA_DEFAULT_CLIENT_TYPE", &default_client_type)?;

        let graph_enabled = match get(vars, "PONTIA_GRAPH_ENABLED") {
            Some(value) => parse_bool("PONTIA_GRAPH_ENABLED", value)?,
            None => true,
        };
        let graph = GraphRuntimeConfig {
            enabled: graph_enabled,
            db_dir: get(vars, "PONTIA_GRAPH_DB_DIR")
                .filter(|value| !value.trim().is_empty())
                .map(ToString::to_string)
                .or_else(|| graph_enabled.then(|| default_graph_db_dir(&database_url))),
        };

        let workspace_browser = match get(vars, "PONTIA_WORKSPACE_ROOTS") {
            Some(value) => WorkspaceBrowserConfig {
                roots: parse_workspace_roots(value)?,
            },
            None => file
                .and_then(|config| config.workspace_browser.clone())
                .unwrap_or_default(),
        };

        let file_picker = file
            .and_then(|config| config.file_picker.clone())
            .unwrap_or_default();

        let mut dashboard = file
            .and_then(|config| config.dashboard.clone())
            .unwrap_or_default();
        if let Some(value) = get(vars, "PONTIA_DASHBOARD_SOURCE") {
            dashboard.source = non_empty(value);
        }
        if let Some(value) = get(vars, "PONTIA_DASHBOARD_CACHE_DIR") {
            dashboard.cache_dir = non_empty(value);
        }

        let mut runtime = file
            .and_then(|config| config.runtime.clone())
            .unwrap_or_default();
        apply_runtime_env_overrides(vars, &mut runtime);

        Ok(Self {
            bind_addr,
            database_url,
            external_api_token,
            run_migrations,
            default_client_type,
            graph,
            workspace_browser,
            file_picker,
            runtime,
            dashboard,
        })
    }
}

fn get<'a>(vars: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    vars.get(key).map(String::as_str)
}

fn validate_real_default_client_type(key: &'static str, client_type: &str) -> Result<()> {
    let expected = agent_clients::default_real_client_type();
    if client_type == expected {
        Ok(())
    } else {
        Err(Error::InvalidConfig {
            key,
            message: format!("default client type must be {expected}, got {client_type}"),
        })
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

fn explicit_config_path(vars: &HashMap<String, String>) -> Option<PathBuf> {
    get(vars, "PONTIA_CONFIG")
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

fn default_config_path_if_exists() -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".config/pontia/config.toml");
    path.exists().then_some(path)
}

fn read_file_config(path: &Path) -> Result<FileConfig> {
    let contents = std::fs::read_to_string(path).map_err(|err| Error::InvalidConfig {
        key: "PONTIA_CONFIG",
        message: format!("failed to read {}: {err}", path.display()),
    })?;
    toml::from_str(&contents).map_err(|err| Error::InvalidConfig {
        key: "PONTIA_CONFIG",
        message: format!("failed to parse {}: {err}", path.display()),
    })
}

fn default_graph_db_dir(database_url: &str) -> String {
    let path = database_url
        .strip_prefix("sqlite://")
        .unwrap_or(database_url);
    let parent = std::path::Path::new(path)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    parent.join("graph").join("lbug").display().to_string()
}

fn parse_workspace_roots(value: &str) -> Result<Vec<WorkspaceRootConfig>> {
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

fn parse_bool(key: &'static str, value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(Error::InvalidConfig {
            key,
            message: format!("expected boolean, got {value:?}"),
        }),
    }
}

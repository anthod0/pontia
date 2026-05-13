use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    application::{
        GraphRuntimeConfig, PlannerRuntimeConfig, WorkspaceBrowserConfig, WorkspaceRootConfig,
    },
    error::{Error, Result},
};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_DATABASE_URL: &str = "sqlite://~/.local/share/llmparty/llmparty.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub external_api_token: Option<String>,
    pub run_migrations: bool,
    pub planner: PlannerRuntimeConfig,
    pub graph: GraphRuntimeConfig,
    pub workspace_browser: WorkspaceBrowserConfig,
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
    #[serde(default)]
    pub pi: RuntimeClientConfig,
    #[serde(default)]
    pub claude_code: RuntimeClientConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct RuntimeClientConfig {
    pub tui_command: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    bind_addr: Option<String>,
    database_url: Option<String>,
    external_api_token: Option<String>,
    run_migrations: Option<bool>,
    runtime: Option<RuntimeConfig>,
    workspace_browser: Option<WorkspaceBrowserConfig>,
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

        let bind_addr = get(vars, "LLMPARTY_BIND_ADDR")
            .or_else(|| file.and_then(|config| config.bind_addr.as_deref()))
            .unwrap_or(DEFAULT_BIND_ADDR)
            .parse::<SocketAddr>()
            .map_err(|err| Error::InvalidConfig {
                key: "LLMPARTY_BIND_ADDR",
                message: err.to_string(),
            })?;

        let database_url = get(vars, "LLMPARTY_DATABASE_URL")
            .or_else(|| file.and_then(|config| config.database_url.as_deref()))
            .unwrap_or(DEFAULT_DATABASE_URL)
            .to_string();

        let external_api_token = get(vars, "LLMPARTY_EXTERNAL_API_TOKEN")
            .or_else(|| file.and_then(|config| config.external_api_token.as_deref()))
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string);

        let run_migrations = match get(vars, "LLMPARTY_RUN_MIGRATIONS") {
            Some(value) => parse_bool("LLMPARTY_RUN_MIGRATIONS", value)?,
            None => file
                .and_then(|config| config.run_migrations)
                .unwrap_or(true),
        };

        let planner = PlannerRuntimeConfig {
            enabled: match get(vars, "LLMPARTY_PLANNER_ENABLED") {
                Some(value) => parse_bool("LLMPARTY_PLANNER_ENABLED", value)?,
                None => false,
            },
            client_type: get(vars, "LLMPARTY_PLANNER_CLIENT_TYPE")
                .unwrap_or("pi")
                .to_string(),
            timeout_ms: match get(vars, "LLMPARTY_PLANNER_TIMEOUT_MS") {
                Some(value) => value.parse::<u64>().map_err(|err| Error::InvalidConfig {
                    key: "LLMPARTY_PLANNER_TIMEOUT_MS",
                    message: err.to_string(),
                })?,
                None => 30_000,
            },
            compatibility_direct_dispatch: match get(
                vars,
                "LLMPARTY_PLANNER_COMPAT_DIRECT_DISPATCH",
            ) {
                Some(value) => parse_bool("LLMPARTY_PLANNER_COMPAT_DIRECT_DISPATCH", value)?,
                None => false,
            },
        };

        let graph_enabled = match get(vars, "LLMPARTY_GRAPH_ENABLED") {
            Some(value) => parse_bool("LLMPARTY_GRAPH_ENABLED", value)?,
            None => false,
        };
        let graph = GraphRuntimeConfig {
            enabled: graph_enabled,
            db_dir: get(vars, "LLMPARTY_GRAPH_DB_DIR")
                .filter(|value| !value.trim().is_empty())
                .map(ToString::to_string)
                .or_else(|| graph_enabled.then(|| default_graph_db_dir(&database_url))),
        };

        let workspace_browser = match get(vars, "LLMPARTY_WORKSPACE_ROOTS") {
            Some(value) => WorkspaceBrowserConfig {
                roots: parse_workspace_roots(value)?,
            },
            None => file
                .and_then(|config| config.workspace_browser.clone())
                .unwrap_or_default(),
        };

        let mut dashboard = file
            .and_then(|config| config.dashboard.clone())
            .unwrap_or_default();
        if let Some(value) = get(vars, "LLMPARTY_DASHBOARD_SOURCE") {
            dashboard.source = non_empty(value);
        }
        if let Some(value) = get(vars, "LLMPARTY_DASHBOARD_CACHE_DIR") {
            dashboard.cache_dir = non_empty(value);
        }

        let mut runtime = file
            .and_then(|config| config.runtime.clone())
            .unwrap_or_default();
        if let Some(value) = get(vars, "LLMPARTY_PI_TUI_COMMAND") {
            runtime.pi.tui_command = non_empty(value);
        }
        if let Some(value) = get(vars, "LLMPARTY_CLAUDE_TUI_COMMAND") {
            runtime.claude_code.tui_command = non_empty(value);
        }

        Ok(Self {
            bind_addr,
            database_url,
            external_api_token,
            run_migrations,
            planner,
            graph,
            workspace_browser,
            runtime,
            dashboard,
        })
    }
}

fn get<'a>(vars: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    vars.get(key).map(String::as_str)
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

fn explicit_config_path(vars: &HashMap<String, String>) -> Option<PathBuf> {
    get(vars, "LLMPARTY_CONFIG")
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
}

fn default_config_path_if_exists() -> Option<PathBuf> {
    let home = env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".config/llmparty/config.toml");
    path.exists().then_some(path)
}

fn read_file_config(path: &Path) -> Result<FileConfig> {
    let contents = std::fs::read_to_string(path).map_err(|err| Error::InvalidConfig {
        key: "LLMPARTY_CONFIG",
        message: format!("failed to read {}: {err}", path.display()),
    })?;
    toml::from_str(&contents).map_err(|err| Error::InvalidConfig {
        key: "LLMPARTY_CONFIG",
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
                    key: "LLMPARTY_WORKSPACE_ROOTS",
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
                    key: "LLMPARTY_WORKSPACE_ROOTS",
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

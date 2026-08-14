use std::{collections::HashMap, env, net::SocketAddr};

use pontia_agent_clients as agent_clients;
use pontia_core::error::{Error, Result};

use super::{
    AppConfig, WorkspaceBrowserConfig,
    environment::{
        apply_runtime_overrides, get, non_empty, parse_bool, validate_real_default_client_type,
    },
    file_config,
    paths::{config_path, default_database_url, pontia_home},
    workspace_roots,
};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let vars: HashMap<String, String> = env::vars().collect();
        Self::from_vars(&vars)
    }

    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self> {
        let pontia_home = pontia_home(vars)?;
        let config_path = config_path(&pontia_home);
        let file = if config_path.exists() {
            Some(file_config::read(&config_path)?)
        } else {
            None
        };
        let file = file.as_ref();

        let bind_addr = file
            .and_then(|config| config.bind_addr.as_deref())
            .unwrap_or(DEFAULT_BIND_ADDR)
            .parse::<SocketAddr>()
            .map_err(|err| Error::InvalidConfig {
                key: "bind_addr",
                message: err.to_string(),
            })?;

        let database_url = default_database_url(&pontia_home);

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

        let workspace_browser = match get(vars, "PONTIA_WORKSPACE_ROOTS") {
            Some(value) => WorkspaceBrowserConfig {
                roots: workspace_roots::parse(value)?,
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

        let mut runtime = file
            .and_then(|config| config.runtime.clone())
            .unwrap_or_default();
        apply_runtime_overrides(vars, &mut runtime);

        Ok(Self {
            pontia_home,
            bind_addr,
            database_url,
            external_api_token,
            run_migrations,
            default_client_type,
            workspace_browser,
            file_picker,
            runtime,
            dashboard,
        })
    }
}

use std::{collections::HashMap, env, net::SocketAddr};

use crate::error::{Error, Result};

const DEFAULT_BIND_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_DATABASE_URL: &str = "sqlite://./data/llmparty.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub bind_addr: SocketAddr,
    pub database_url: String,
    pub external_api_token: Option<String>,
    pub run_migrations: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();
        let vars: HashMap<String, String> = env::vars().collect();
        Self::from_vars(&vars)
    }

    pub fn from_vars(vars: &HashMap<String, String>) -> Result<Self> {
        let bind_addr = get(vars, "LLMPARTY_BIND_ADDR")
            .unwrap_or(DEFAULT_BIND_ADDR)
            .parse::<SocketAddr>()
            .map_err(|err| Error::InvalidConfig {
                key: "LLMPARTY_BIND_ADDR",
                message: err.to_string(),
            })?;

        let database_url = get(vars, "LLMPARTY_DATABASE_URL")
            .unwrap_or(DEFAULT_DATABASE_URL)
            .to_string();

        let external_api_token = get(vars, "LLMPARTY_EXTERNAL_API_TOKEN")
            .filter(|value| !value.trim().is_empty())
            .map(ToString::to_string);

        let run_migrations = match get(vars, "LLMPARTY_RUN_MIGRATIONS") {
            Some(value) => parse_bool("LLMPARTY_RUN_MIGRATIONS", value)?,
            None => true,
        };

        Ok(Self {
            bind_addr,
            database_url,
            external_api_token,
            run_migrations,
        })
    }
}

fn get<'a>(vars: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    vars.get(key).map(String::as_str)
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

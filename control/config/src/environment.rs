use std::collections::HashMap;

use pontia_agent_clients as agent_clients;
use pontia_core::error::{Error, Result};

use super::RuntimeConfig;

pub(super) fn apply_runtime_overrides(vars: &HashMap<String, String>, runtime: &mut RuntimeConfig) {
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

pub(super) fn get<'a>(vars: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    vars.get(key).map(String::as_str)
}

pub(super) fn validate_real_default_client_type(
    key: &'static str,
    client_type: &str,
) -> Result<()> {
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

pub(super) fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

pub(super) fn parse_bool(key: &'static str, value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(Error::InvalidConfig {
            key,
            message: format!("expected boolean, got {value:?}"),
        }),
    }
}

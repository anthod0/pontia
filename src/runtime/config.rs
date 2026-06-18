use std::{
    net::SocketAddr,
    sync::{OnceLock, RwLock},
};

use crate::{agent_clients, config::RuntimeConfig};

fn runtime_config() -> &'static RwLock<RuntimeConfig> {
    static CONFIG: OnceLock<RwLock<RuntimeConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| RwLock::new(RuntimeConfig::default()))
}

fn external_api_token_config() -> &'static RwLock<Option<String>> {
    static CONFIG: OnceLock<RwLock<Option<String>>> = OnceLock::new();
    CONFIG.get_or_init(|| RwLock::new(None))
}

fn runtime_bind_addr_config() -> &'static RwLock<Option<SocketAddr>> {
    static CONFIG: OnceLock<RwLock<Option<SocketAddr>>> = OnceLock::new();
    CONFIG.get_or_init(|| RwLock::new(None))
}

pub fn set_runtime_config(config: RuntimeConfig) {
    let mut guard = runtime_config()
        .write()
        .expect("runtime config lock poisoned");
    *guard = config;
}

pub fn set_runtime_external_api_token(token: Option<String>) {
    let mut guard = external_api_token_config()
        .write()
        .expect("runtime external api token lock poisoned");
    *guard = token;
}

pub fn set_runtime_bind_addr(bind_addr: SocketAddr) {
    let mut guard = runtime_bind_addr_config()
        .write()
        .expect("runtime bind addr lock poisoned");
    *guard = Some(bind_addr);
}

#[cfg(test)]
pub fn reset_runtime_bind_addr_for_tests() {
    let mut guard = runtime_bind_addr_config()
        .write()
        .expect("runtime bind addr lock poisoned");
    *guard = None;
}

pub(super) fn configured_external_api_token() -> Option<String> {
    external_api_token_config()
        .read()
        .expect("runtime external api token lock poisoned")
        .clone()
}

pub fn configured_internal_event_url() -> Option<String> {
    configured_api_base_url().map(|base_url| format!("{base_url}/internal/v1/events"))
}

pub fn configured_external_api_url() -> Option<String> {
    configured_api_base_url().map(|base_url| format!("{base_url}/external/v1"))
}

fn configured_api_base_url() -> Option<String> {
    let guard = runtime_bind_addr_config()
        .read()
        .expect("runtime bind addr lock poisoned");
    guard.map(|bind_addr| format!("http://127.0.0.1:{}", bind_addr.port()))
}

pub(super) fn configured_tui_command(client_type: &str) -> Option<String> {
    let guard = runtime_config()
        .read()
        .expect("runtime config lock poisoned");
    let runtime_config_key = agent_clients::get_client_definition(client_type)?
        .tmux_runtime()?
        .runtime_config_key?;
    match runtime_config_key {
        "pi" => guard.pi.tui_command.clone(),
        "claude_code" => guard.claude_code.tui_command.clone(),
        _ => None,
    }
}

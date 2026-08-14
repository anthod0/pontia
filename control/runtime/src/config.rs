use std::{
    net::SocketAddr,
    sync::{OnceLock, RwLock},
};

use pontia_agent_clients as agent_clients;
use pontia_config::RuntimeConfig;

fn runtime_config() -> &'static RwLock<RuntimeConfig> {
    static CONFIG: OnceLock<RwLock<RuntimeConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| RwLock::new(RuntimeConfig::default()))
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

pub fn configured_internal_event_url() -> Option<String> {
    configured_api_base_url().map(|base_url| format!("{base_url}/internal/v1/events"))
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
    let runtime_config_key = agent_clients::get_client_spec(client_type)?
        .tmux_runtime()?
        .runtime_config_key?;
    guard.tui_command_for_client_config_key(runtime_config_key)
}

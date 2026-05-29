use std::sync::{OnceLock, RwLock};

use crate::{agent_clients, config::RuntimeConfig};

fn runtime_config() -> &'static RwLock<RuntimeConfig> {
    static CONFIG: OnceLock<RwLock<RuntimeConfig>> = OnceLock::new();
    CONFIG.get_or_init(|| RwLock::new(RuntimeConfig::default()))
}

pub fn set_runtime_config(config: RuntimeConfig) {
    let mut guard = runtime_config()
        .write()
        .expect("runtime config lock poisoned");
    *guard = config;
}

pub(super) fn configured_tui_command(client_type: &str) -> Option<String> {
    let guard = runtime_config()
        .read()
        .expect("runtime config lock poisoned");
    let runtime_config_key = agent_clients::get_client_spec(client_type)?
        .tmux_runtime()?
        .runtime_config_key?;
    match runtime_config_key {
        "pi" => guard.pi.tui_command.clone(),
        "claude_code" => guard.claude_code.tui_command.clone(),
        _ => None,
    }
}

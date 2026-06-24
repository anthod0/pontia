use std::sync::{OnceLock, RwLock};

use pontia_agent_clients as agent_clients;

fn default_client_type_store() -> &'static RwLock<String> {
    static DEFAULT_CLIENT_TYPE: OnceLock<RwLock<String>> = OnceLock::new();
    DEFAULT_CLIENT_TYPE
        .get_or_init(|| RwLock::new(agent_clients::default_real_client_type().to_string()))
}

pub(crate) fn set_default_client_type(client_type: String) {
    let mut guard = default_client_type_store()
        .write()
        .expect("default client type lock poisoned");
    *guard = client_type;
}

pub(crate) fn default_client_type() -> String {
    default_client_type_store()
        .read()
        .expect("default client type lock poisoned")
        .clone()
}

pub(crate) fn is_supported_client_type(client_type: &str) -> bool {
    agent_clients::is_supported_client_type(client_type)
}

use std::sync::{OnceLock, RwLock};

fn default_client_type_store() -> &'static RwLock<String> {
    static DEFAULT_CLIENT_TYPE: OnceLock<RwLock<String>> = OnceLock::new();
    DEFAULT_CLIENT_TYPE.get_or_init(|| RwLock::new("pi".to_string()))
}

pub fn set_default_client_type(client_type: String) {
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

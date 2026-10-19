use std::{collections::HashMap, fs, io::ErrorKind};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use pontia_config::AppConfig;
use toml_edit::DocumentMut;

use crate::private_file;

/// Load only configuration that the installed service will use.
pub fn load(vars: &HashMap<String, String>) -> Result<AppConfig, String> {
    let persistent_vars = ["HOME", "PONTIA_HOME"]
        .into_iter()
        .filter_map(|key| vars.get(key).map(|value| (key.to_string(), value.clone())))
        .collect();
    pontia_clients::config_from_vars(&persistent_vars).map_err(|_| {
        "failed to load Pontia configuration; verify PONTIA_HOME, config.toml syntax, and configured values"
            .to_string()
    })
}

pub fn generate_token(
    fill_random: impl FnOnce(&mut [u8]) -> Result<(), String>,
) -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    fill_random(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

/// Persist the credential without materializing configuration defaults.
pub fn persist_token(config: &AppConfig, token: &str) -> Result<bool, String> {
    let path = config.pontia_home.join("config.toml");
    let original = match fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    let mut document = original
        .parse::<DocumentMut>()
        .map_err(|_| format!("failed to parse Pontia configuration at {}", path.display()))?;
    if config.external_api_token.as_deref() != Some(token)
        || document.get("external_api_token").is_none()
    {
        document["external_api_token"] = toml_edit::value(token);
    }
    let updated = document.to_string();
    private_file::atomic_write(&path, updated.as_bytes())?;
    Ok(updated != original)
}

pub fn prepare(
    vars: &HashMap<String, String>,
    fill_random: impl FnOnce(&mut [u8]) -> Result<(), String>,
) -> Result<AppConfig, String> {
    let config = load(vars)?;
    if config.bind_addr.port() == 0 {
        return Err(
            "bind_addr port 0 is not supported by pontia up; configure a concrete service port"
                .to_string(),
        );
    }
    let token = match config.external_api_token.as_ref() {
        Some(token) => token.clone(),
        None => generate_token(fill_random)?,
    };
    persist_token(&config, &token)?;
    load(vars)
}

pub fn dashboard_url(config: &AppConfig) -> Result<String, String> {
    let addr = crate::lifecycle::local_health_addr(config.bind_addr);
    let token = config
        .external_api_token
        .as_deref()
        .ok_or_else(|| "local External API token is missing".to_string())?;
    let mut url = url::Url::parse(&format!("http://{addr}/dashboard"))
        .map_err(|error| format!("failed to build Dashboard URL: {error}"))?;
    url.query_pairs_mut().append_pair("token", token);
    Ok(url.to_string())
}

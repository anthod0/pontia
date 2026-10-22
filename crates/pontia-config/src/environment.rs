use std::collections::HashMap;

use pontia_core::error::{Error, Result};

pub(super) fn get<'a>(vars: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    vars.get(key).map(String::as_str)
}

pub(super) fn validate_real_default_client_type(
    key: &'static str,
    client_type: &str,
    expected: &str,
) -> Result<()> {
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

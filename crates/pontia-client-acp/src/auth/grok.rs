use std::{collections::HashMap, path::PathBuf};

use pontia_agent_control::client_contract::auth::{
    AuthAccountInfo, AuthMethodDescriptor, AuthMethodKind, AuthState, ClientAuthStatus,
    StartLoginParams,
};
use pontia_core::{Error, Result};
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const DEVICE_AUTH: &str = "grok_device_auth";

pub(super) fn methods() -> Vec<AuthMethodDescriptor> {
    vec![AuthMethodDescriptor {
        id: DEVICE_AUTH.into(),
        label: "Grok Account (Device Code)".into(),
        description: None,
        kind: AuthMethodKind::DeviceCode,
        recommended: true,
        remote_compatible: true,
        start_fields: Vec::new(),
    }]
}

fn grok_home(vars: &HashMap<String, String>) -> Option<PathBuf> {
    let configured = |name: &str| vars.get(name).filter(|value| !value.is_empty());
    configured("GROK_HOME")
        .map(PathBuf::from)
        .or_else(|| configured("HOME").map(|home| PathBuf::from(home).join(".grok")))
}

/// Grok has no status command. Its credential file is read without network access,
/// and secret fields are only checked for presence.
pub(super) fn status(vars: &HashMap<String, String>) -> ClientAuthStatus {
    let unknown = |detail: &str| {
        ClientAuthStatus::new(AuthState::Unknown, methods(), true).with_detail(detail)
    };
    let mut status = ClientAuthStatus::new(AuthState::Unauthenticated, methods(), true);
    let Some(home) = grok_home(vars) else {
        return unknown("Cannot locate the Grok home directory");
    };
    match std::fs::read(home.join("auth.json")) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(entries)) => {
                if let Some(entry) = entries.values().find(|entry| has_credential(entry)) {
                    let text = |field: &str| {
                        entry[field]
                            .as_str()
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned)
                    };
                    let expires_at = text("expires_at");
                    let expired = expires_at
                        .as_deref()
                        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
                        .is_some_and(|expiry| expiry <= OffsetDateTime::now_utc());
                    status.state = if expired && text("refresh_token").is_none() {
                        AuthState::Expired
                    } else {
                        AuthState::Authenticated
                    };
                    status.method_id = Some(DEVICE_AUTH.into());
                    status.account = Some(AuthAccountInfo {
                        label: text("email"),
                        plan: text("auth_mode"),
                        provider: Some("xai".into()),
                        workspace: text("team_id"),
                        expires_at,
                    });
                    return status;
                }
            }
            Ok(_) | Err(_) => return unknown("Cannot read the Grok credential file"),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return unknown("Cannot read the Grok credential file"),
    }
    if vars.get("XAI_API_KEY").is_some_and(|key| !key.is_empty()) {
        status.state = AuthState::Authenticated;
        status.method_id = Some("env_api_key".into());
        status.account = Some(AuthAccountInfo {
            label: Some("XAI_API_KEY".into()),
            provider: Some("xai".into()),
            ..AuthAccountInfo::default()
        });
        status.detail = Some("Authenticated via XAI_API_KEY environment variable".into());
    }
    status
}

fn has_credential(entry: &Value) -> bool {
    ["key", "refresh_token"]
        .iter()
        .any(|field| entry[field].as_str().is_some_and(|value| !value.is_empty()))
}

pub(super) fn login_args(method_id: &str, params: &StartLoginParams) -> Result<Vec<String>> {
    if method_id != DEVICE_AUTH {
        return Err(Error::Domain(format!(
            "Unsupported Grok login method: {method_id}"
        )));
    }
    if let Some(field) = params.fields.keys().next() {
        return Err(Error::Domain(format!("Unknown login field: {field}")));
    }
    Ok(vec!["login".into(), "--device-auth".into()])
}

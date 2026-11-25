use std::process::Output;

use pontia_agent_control::{
    client_auth::sanitize_output,
    client_contract::auth::{
        AuthAccountInfo, AuthFieldDescriptor, AuthFieldKind, AuthMethodDescriptor, AuthMethodKind,
        AuthState, ClientAuthStatus, StartLoginParams,
    },
};
use pontia_core::{Error, Result};
use serde_json::Value;

pub(super) const DEVICE_CODE_LIFETIME_SECS: u64 = 600;
const FREE: &str = "kiro_device_free";
const PRO: &str = "kiro_device_pro";
const DEFAULT_REGION: &str = "us-east-1";

pub(super) fn methods() -> Vec<AuthMethodDescriptor> {
    vec![
        AuthMethodDescriptor {
            id: FREE.into(),
            label: "Kiro Free (AWS Builder ID)".into(),
            description: None,
            kind: AuthMethodKind::DeviceCode,
            recommended: true,
            remote_compatible: true,
            start_fields: Vec::new(),
        },
        AuthMethodDescriptor {
            id: PRO.into(),
            label: "Kiro Pro (IAM Identity Center)".into(),
            description: None,
            kind: AuthMethodKind::DeviceCode,
            recommended: false,
            remote_compatible: true,
            start_fields: vec![
                AuthFieldDescriptor {
                    name: "identity_provider".into(),
                    label: "Identity provider URL".into(),
                    kind: AuthFieldKind::Url,
                    required: true,
                    placeholder: Some("https://d-xxxxxxxxxx.awsapps.com/start".into()),
                    default_value: None,
                    description: None,
                },
                AuthFieldDescriptor {
                    name: "region".into(),
                    label: "Region".into(),
                    kind: AuthFieldKind::Text,
                    required: true,
                    placeholder: None,
                    default_value: Some(DEFAULT_REGION.into()),
                    description: None,
                },
            ],
        },
    ]
}

/// `whoami --format json` prints a JSON object followed by plain-text profile details.
pub(super) fn status(output: &Output) -> ClientAuthStatus {
    let first = serde_json::Deserializer::from_slice(&output.stdout)
        .into_iter::<Value>()
        .next()
        .and_then(|value| value.ok())
        .filter(Value::is_object);
    let signed_out = first
        .as_ref()
        .is_some_and(|value| value.get("account").is_some_and(Value::is_null));
    let mut status = ClientAuthStatus::new(AuthState::Unknown, methods(), true);
    if output.status.success()
        && !signed_out
        && let Some(account) = &first
    {
        let text = |field: &str| account[field].as_str().map(str::to_owned);
        let plan = text("accountType");
        status.state = AuthState::Authenticated;
        status.method_id = Some(
            if plan.as_deref() == Some("IamIdentityCenter") {
                PRO
            } else {
                FREE
            }
            .into(),
        );
        status.account = Some(AuthAccountInfo {
            label: text("email")
                .or_else(|| text("startUrl"))
                .or_else(|| plan.clone()),
            plan,
            provider: Some("aws".into()),
            workspace: text("startUrl"),
            expires_at: None,
        });
        return status;
    }
    let text = super::combined_output(output);
    let lower = text.to_ascii_lowercase();
    if signed_out
        || ["not logged in", "no active", "unauthenticated"]
            .iter()
            .any(|marker| lower.contains(marker))
    {
        status.state = AuthState::Unauthenticated;
    } else if lower.contains("expired") {
        status.state = AuthState::Expired;
    } else {
        status.detail = Some(sanitize_output(&format!(
            "kiro-cli whoami exited with {}: {text}",
            output.status
        )));
    }
    status
}

pub(super) fn login_args(method_id: &str, params: &StartLoginParams) -> Result<Vec<String>> {
    let allowed: &[&str] = match method_id {
        FREE => &[],
        PRO => &["identity_provider", "region"],
        _ => {
            return Err(Error::Domain(format!(
                "Unsupported Kiro login method: {method_id}"
            )));
        }
    };
    if let Some(field) = params
        .fields
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(Error::Domain(format!("Unknown login field: {field}")));
    }
    let mut args = vec!["login".to_owned(), "--license".to_owned()];
    if method_id == FREE {
        args.push("free".into());
    } else {
        let field = |name: &str| {
            params
                .fields
                .get(name)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| Error::Domain(format!("{name} is required")))
        };
        let identity_provider = field("identity_provider")?;
        let region = field("region")?;
        if !valid_start_url(identity_provider) {
            return Err(Error::Domain(
                "identity_provider must be an https:// URL".into(),
            ));
        }
        if !valid_region(region) {
            return Err(Error::Domain(
                "region must be an AWS Region such as us-east-1".into(),
            ));
        }
        args.extend([
            "pro".into(),
            "--identity-provider".into(),
            identity_provider.into(),
            "--region".into(),
            region.into(),
        ]);
    }
    args.push("--use-device-flow".into());
    Ok(args)
}

fn valid_start_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.is_empty()
        && !host.contains('@')
        && url.len() <= 2048
        && !url
            .chars()
            .any(|char| char.is_whitespace() || char.is_control())
}

/// `^[a-z]{2}(-[a-z0-9]+)+$`
fn valid_region(region: &str) -> bool {
    let mut parts = region.split('-');
    let prefix = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    prefix.len() == 2
        && prefix.chars().all(|char| char.is_ascii_lowercase())
        && !rest.is_empty()
        && rest.iter().all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|char| char.is_ascii_lowercase() || char.is_ascii_digit())
        })
}

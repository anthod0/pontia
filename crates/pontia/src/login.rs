use std::{
    collections::HashMap,
    path::{Component, Path, PathBuf},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use pontia::private_file;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, sleep};
use uuid::Uuid;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const SLOW_DOWN_SECONDS: u64 = 5;

#[derive(Debug, Deserialize)]
struct AuthorizationResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    token_type: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct StoredCredential<'a> {
    #[serde(borrow)]
    token: &'a str,
}

pub async fn run(vars: &HashMap<String, String>) -> Result<(), String> {
    run_with_verification(vars, |_| {}).await
}

pub async fn run_with_verification<F>(
    vars: &HashMap<String, String>,
    on_verification: F,
) -> Result<(), String>
where
    F: FnOnce(&str),
{
    let home = pontia_home(vars)?;
    let origin = auth_origin(vars)?;
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| format!("failed to create the login HTTP client: {error}"))?;

    tokio::select! {
        result = login(&client, &origin, &home, on_verification) => result,
        result = tokio::signal::ctrl_c() => {
            result.map_err(|error| format!("failed to listen for cancellation: {error}"))?;
            Err("login canceled".to_string())
        }
    }
}

async fn login<F>(
    client: &Client,
    origin: &Url,
    home: &Path,
    on_verification: F,
) -> Result<(), String>
where
    F: FnOnce(&str),
{
    let authorization_url = origin
        .join("api/device/authorize")
        .map_err(|error| format!("invalid authorization endpoint: {error}"))?;
    let authorization = client
        .post(authorization_url)
        .send()
        .await
        .map_err(|error| format!("failed to start login: {error}"))?;
    if !authorization.status().is_success() {
        return Err(format!(
            "the login service rejected the authorization request ({})",
            authorization.status()
        ));
    }
    let authorization: AuthorizationResponse = authorization
        .json()
        .await
        .map_err(|_| "the login service returned an invalid authorization response".to_string())?;
    let verification_url = validate_authorization(&authorization, origin)?;

    println!("Open this address on a device with a browser:");
    println!("{verification_url}");
    println!();
    println!("Enter this code:");
    println!("{}", authorization.user_code);
    println!();
    println!("Waiting for approval...");
    on_verification(verification_url.as_str());

    let token_url = origin
        .join("api/device/token")
        .map_err(|error| format!("invalid token endpoint: {error}"))?;
    let deadline = Instant::now() + Duration::from_secs(authorization.expires_in);
    let mut interval = Duration::from_secs(authorization.interval);
    loop {
        if Instant::now() + interval >= deadline {
            return Err("the login request expired".to_string());
        }
        sleep(interval).await;
        let response = client
            .post(token_url.clone())
            .json(&serde_json::json!({ "device_code": authorization.device_code }))
            .send()
            .await
            .map_err(|error| format!("failed to poll the login service: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "the login service rejected the polling request ({})",
                response.status()
            ));
        }
        let response: TokenResponse = response
            .json()
            .await
            .map_err(|_| "the login service returned an invalid polling response".to_string())?;
        match (response.access_token, response.error.as_deref()) {
            (Some(token), None) if response.token_type.as_deref() == Some("Bearer") => {
                validate_token(&token)?;
                write_credential(&home.join("auth.json"), &token)?;
                println!(
                    "Login complete. Credential saved to {}.",
                    home.join("auth.json").display()
                );
                return Ok(());
            }
            (None, Some("authorization_pending")) => {}
            (None, Some("slow_down")) => interval += Duration::from_secs(SLOW_DOWN_SECONDS),
            (None, Some("access_denied")) => return Err("login was denied".to_string()),
            (None, Some("expired_token")) => return Err("the login request expired".to_string()),
            _ => return Err("the login service returned an invalid polling response".to_string()),
        }
    }
}

fn validate_authorization(response: &AuthorizationResponse, origin: &Url) -> Result<Url, String> {
    let mut verification = Url::parse(&response.verification_uri)
        .map_err(|_| "the login service returned an invalid verification address".to_string())?;
    if verification.origin() != origin.origin()
        || response.expires_in == 0
        || response.expires_in > 3600
        || response.interval == 0
        || response.interval > response.expires_in
        || !valid_device_code(&response.device_code)
        || !valid_user_code(&response.user_code)
    {
        return Err("the login service returned an invalid authorization response".to_string());
    }
    verification
        .query_pairs_mut()
        .append_pair("user_code", &response.user_code);
    Ok(verification)
}

fn valid_device_code(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn valid_user_code(value: &str) -> bool {
    value.len() == 9
        && value.as_bytes()[4] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || b"BCDFGHJKLMNPQRSTVWXZ".contains(&byte))
}

pub(super) fn validate_token(token: &str) -> Result<(), String> {
    let Some(value) = token.strip_prefix("ptr_v1_") else {
        return Err("the login service returned an invalid credential".to_string());
    };
    let Some((session_id, secret)) = value.split_once('_') else {
        return Err("the login service returned an invalid credential".to_string());
    };
    let valid_session_id = Uuid::parse_str(session_id)
        .is_ok_and(|parsed| parsed.get_version_num() == 7 && parsed.to_string() == session_id);
    let valid_secret = URL_SAFE_NO_PAD
        .decode(secret)
        .is_ok_and(|decoded| decoded.len() == 32 && URL_SAFE_NO_PAD.encode(decoded) == secret);
    if !valid_session_id || !valid_secret {
        return Err("the login service returned an invalid credential".to_string());
    }
    Ok(())
}

pub(super) fn auth_origin(vars: &HashMap<String, String>) -> Result<Url, String> {
    let origin = pontia_config::auth_origin(vars).map_err(|error| error.to_string())?;
    Url::parse(&origin).map_err(|error| format!("PONTIA_AUTH_ORIGIN is invalid: {error}"))
}

pub(super) fn has_valid_credential(vars: &HashMap<String, String>) -> Result<bool, String> {
    let path = pontia_home(vars)?.join("auth.json");
    let contents = match std::fs::read(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    let Ok(stored) = serde_json::from_slice::<StoredCredential<'_>>(&contents) else {
        return Ok(false);
    };
    Ok(validate_token(stored.token).is_ok())
}

pub(super) fn pontia_home(vars: &HashMap<String, String>) -> Result<PathBuf, String> {
    let (key, value, append) = match vars.get("PONTIA_HOME") {
        Some(value) => ("PONTIA_HOME", value.as_str(), false),
        None => (
            "HOME",
            vars.get("HOME")
                .map(String::as_str)
                .ok_or_else(|| "PONTIA_HOME must be set when HOME is unavailable".to_string())?,
            true,
        ),
    };
    let path = PathBuf::from(value);
    if value.trim().is_empty()
        || !path.is_absolute()
        || path.parent().is_none()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!(
            "{key} must be a non-root absolute path without parent traversal"
        ));
    }
    Ok(if append { path.join(".pontia") } else { path })
}

fn write_credential(path: &Path, token: &str) -> Result<(), String> {
    let mut contents = serde_json::to_vec_pretty(&StoredCredential { token })
        .map_err(|error| format!("failed to serialize login credential: {error}"))?;
    contents.push(b'\n');
    private_file::atomic_write(path, &contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_credential_with_underscore_in_secret() {
        let secret = URL_SAFE_NO_PAD.encode([u8::MAX; 32]);
        assert!(secret.contains('_'));

        validate_token(&format!(
            "ptr_v1_0199791c-6600-7000-8000-000000000001_{secret}"
        ))
        .unwrap();
    }

    #[test]
    fn rejects_malformed_credential() {
        assert!(validate_token("ptr_v1_not-a-uuid_secret").is_err());
    }

    #[test]
    fn verification_url_includes_the_user_code() {
        let origin = Url::parse("https://pontia.dev").unwrap();
        let authorization = AuthorizationResponse {
            device_code: "a".repeat(43),
            user_code: "BCDF-GHJK".to_string(),
            verification_uri: "https://pontia.dev/device".to_string(),
            expires_in: 300,
            interval: 5,
        };

        let verification = validate_authorization(&authorization, &origin).unwrap();

        assert_eq!(
            verification.as_str(),
            "https://pontia.dev/device?user_code=BCDF-GHJK"
        );
    }

    #[test]
    fn detects_a_valid_saved_credential_without_using_the_process_home() {
        let root = tempfile::tempdir().unwrap();
        let vars = HashMap::from([("PONTIA_HOME".to_string(), root.path().display().to_string())]);
        assert!(!has_valid_credential(&vars).unwrap());

        let secret = URL_SAFE_NO_PAD.encode([7_u8; 32]);
        let token = format!("ptr_v1_0199791c-6600-7000-8000-000000000001_{secret}");
        std::fs::write(
            root.path().join("auth.json"),
            serde_json::json!({ "token": token }).to_string(),
        )
        .unwrap();

        assert!(has_valid_credential(&vars).unwrap());
    }
}

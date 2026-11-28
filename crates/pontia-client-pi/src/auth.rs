use std::{
    collections::{HashMap, HashSet},
    path::Path,
    process::{Output, Stdio},
    time::Duration,
};

use pontia_agent_control::{
    client_auth::sanitize_output,
    client_contract::{
        ClientAuth, ClientOperation,
        auth::{
            AuthAccountInfo, AuthContext, AuthEventSink, AuthFlowHandle, AuthState,
            ClientAuthStatus, StartLoginParams,
        },
    },
};
use pontia_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncWriteExt, BufReader};

const STATUS_TIMEOUT: Duration = if cfg!(test) {
    Duration::from_millis(200)
} else {
    Duration::from_secs(4)
};

pub(crate) struct PiAuth;

impl PiAuth {
    async fn probe(&self, key: &str, context: &AuthContext) -> Result<ClientAuthStatus> {
        if !self.supports_auth(key) {
            return Err(Error::Domain("Pi requires the default client key".into()));
        }
        let providers = available_providers(&context.pontia_home, &context.vars).await?;
        if providers.is_empty() {
            return Ok(status(AuthState::Unauthenticated));
        }

        let mut checks = tokio::task::JoinSet::new();
        for provider in providers {
            let directory = context.pontia_home.clone();
            let vars = context.vars.clone();
            checks.spawn(async move {
                let output = run_auth_check(&provider, &directory, &vars).await?;
                parse_auth_check(&output, &provider)
            });
        }

        let mut ready = Vec::new();
        let mut unavailable = Vec::new();
        while let Some(result) = checks.join_next().await {
            match result.map_err(|error| Error::ControlUnknown(error.to_string()))?? {
                check if check.is_authenticated() => ready.push(check),
                check if check.status == CheckState::NotReady => {}
                check => unavailable.push(check),
            }
        }
        ready.sort_by(|left, right| left.provider.cmp(&right.provider));
        if let Some(primary) = ready.first() {
            let mut result = status(AuthState::Authenticated);
            result.account = Some(AuthAccountInfo {
                label: Some(primary.provider.clone()),
                plan: primary.auth_type.clone(),
                provider: Some(primary.provider.clone()),
                ..AuthAccountInfo::default()
            });
            if ready.len() > 1 {
                result.detail = Some(format!(
                    "Ready providers: {}",
                    ready
                        .iter()
                        .map(AuthCheck::description)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            return Ok(result);
        }
        if unavailable.is_empty() {
            return Ok(status(AuthState::Unauthenticated));
        }
        unavailable.sort_by(|left, right| left.provider.cmp(&right.provider));
        Ok(status(AuthState::Unknown).with_detail(format!(
            "Pi authentication check returned invalid state for {}",
            unavailable
                .iter()
                .map(|check| check.provider.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }
}

fn status(state: AuthState) -> ClientAuthStatus {
    ClientAuthStatus::new(state, Vec::new(), false)
}

impl ClientAuth for PiAuth {
    fn supports_auth(&self, key: &str) -> bool {
        key == "default"
    }

    fn status<'a>(
        &'a self,
        key: &'a str,
        context: &'a AuthContext,
    ) -> ClientOperation<'a, ClientAuthStatus> {
        Box::pin(async move {
            match tokio::time::timeout(STATUS_TIMEOUT, self.probe(key, context)).await {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(error)) => {
                    Ok(status(AuthState::Unknown).with_detail(sanitize_output(&error.to_string())))
                }
                Err(_) => Ok(status(AuthState::Unknown)
                    .with_detail("Pi authentication status check timed out")),
            }
        })
    }

    fn start_login<'a>(
        &'a self,
        _key: &'a str,
        _method_id: &'a str,
        _params: StartLoginParams,
        _context: &'a AuthContext,
        _sink: AuthEventSink,
    ) -> ClientOperation<'a, AuthFlowHandle> {
        Box::pin(async { Err(Error::Unsupported("Pontia does not manage Pi login".into())) })
    }

    fn logout<'a>(
        &'a self,
        _key: &'a str,
        _scope: Option<&'a str>,
        _context: &'a AuthContext,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async {
            Err(Error::Unsupported(
                "Pontia does not manage Pi logout".into(),
            ))
        })
    }
}

#[derive(Deserialize)]
struct AvailableModel {
    provider: String,
}

async fn available_providers(
    directory: &Path,
    vars: &HashMap<String, String>,
) -> Result<Vec<String>> {
    let mut child = tokio::process::Command::new("pi")
        .args([
            "--offline",
            "--no-approve",
            "--no-mcp",
            "--no-skills",
            "--no-prompt-templates",
            "--no-context-files",
            "--mode",
            "rpc",
            "--no-session",
        ])
        .current_dir(directory)
        .env_clear()
        .envs(vars)
        .env("NO_COLOR", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    stdin
        .write_all(format!("{}\n", json!({"id":"models","type":"get_available_models"})).as_bytes())
        .await?;
    let models = loop {
        let frame = pontia_agent_control::ipc::rpc::read_frame(&mut stdout).await?;
        let response: Value = serde_json::from_slice(&frame)?;
        if response["type"] != "response" || response["id"] != "models" {
            continue;
        }
        if response["success"] != true {
            return Err(Error::Unavailable(
                response["error"]
                    .as_str()
                    .unwrap_or("Pi model listing failed")
                    .into(),
            ));
        }
        break serde_json::from_value::<Vec<AvailableModel>>(response["data"]["models"].clone())?;
    };
    drop(stdin);
    if !child.wait().await?.success() {
        return Err(Error::Unavailable("Pi model listing process failed".into()));
    }
    let mut seen = HashSet::new();
    Ok(models
        .into_iter()
        .map(|model| model.provider)
        .filter(|provider| !provider.is_empty() && seen.insert(provider.clone()))
        .collect())
}

async fn run_auth_check(
    provider: &str,
    directory: &Path,
    vars: &HashMap<String, String>,
) -> Result<Output> {
    tokio::process::Command::new("pi")
        .args([
            "auth",
            "check",
            "--provider",
            provider,
            "--json",
            "--no-refresh",
        ])
        .current_dir(directory)
        .env_clear()
        .envs(vars)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(Error::from)
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CheckState {
    Ready,
    NotReady,
    Invalid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthCheck {
    status: CheckState,
    provider: String,
    auth_type: Option<String>,
    reason: Option<CheckReason>,
}

impl AuthCheck {
    fn is_authenticated(&self) -> bool {
        self.status == CheckState::Ready || self.reason == Some(CheckReason::ProviderNotFound)
    }

    fn description(&self) -> String {
        match &self.auth_type {
            Some(auth_type) => format!("{} ({auth_type})", self.provider),
            None => self.provider.clone(),
        }
    }
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum CheckReason {
    ProviderNotFound,
    CredentialsNotConfigured,
    CredentialNotAvailable,
    InvalidState,
}

fn parse_auth_check(output: &Output, expected_provider: &str) -> Result<AuthCheck> {
    let check: AuthCheck = serde_json::from_slice(&output.stdout)?;
    let expected_code = match check.status {
        CheckState::Ready => 0,
        CheckState::NotReady => 1,
        CheckState::Invalid => 2,
    };
    if check.provider != expected_provider || output.status.code() != Some(expected_code) {
        return Err(Error::Unavailable(
            "Pi authentication check returned an inconsistent result".into(),
        ));
    }
    Ok(check)
}

#[cfg(test)]
mod tests;

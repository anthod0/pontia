use std::{process::Stdio, time::Duration};

use pontia_agent_control::client_contract::{
    ClientAuth, ClientOperation,
    auth::{
        AuthAccountInfo, AuthContext, AuthEventSink, AuthFlowHandle, AuthState, ClientAuthStatus,
        StartLoginParams,
    },
};
use pontia_core::Error;
use serde_json::Value;

const STATUS_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct ClaudeAuth {
    status_timeout: Duration,
}

impl Default for ClaudeAuth {
    fn default() -> Self {
        Self {
            status_timeout: STATUS_TIMEOUT,
        }
    }
}

fn unknown(detail: impl Into<String>) -> ClientAuthStatus {
    ClientAuthStatus::new(AuthState::Unknown, Vec::new(), false).with_detail(detail)
}

fn text(value: &Value, fields: &[&str]) -> Option<String> {
    fields.iter().find_map(|field| {
        value
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn parse_status(stdout: &[u8]) -> ClientAuthStatus {
    let Ok(value) = serde_json::from_slice::<Value>(stdout) else {
        return unknown("claude auth status returned invalid JSON");
    };
    let logged_in = value
        .get("loggedIn")
        .or_else(|| value.get("logged_in"))
        .and_then(Value::as_bool);
    match logged_in {
        Some(false) => ClientAuthStatus::new(AuthState::Unauthenticated, Vec::new(), false),
        Some(true) => {
            let mut status = ClientAuthStatus::new(AuthState::Authenticated, Vec::new(), false);
            status.account = Some(AuthAccountInfo {
                label: text(&value, &["email", "orgName", "org", "authMethod"]),
                plan: text(&value, &["subscriptionType", "subscription"]),
                provider: text(&value, &["apiProvider", "provider", "authMethod"]),
                workspace: text(&value, &["orgName", "org"]),
                expires_at: None,
            });
            status
        }
        None => unknown("claude auth status did not report loggedIn"),
    }
}

impl ClientAuth for ClaudeAuth {
    fn supports_auth(&self, key: &str) -> bool {
        key == "default"
    }

    fn status<'a>(
        &'a self,
        _key: &'a str,
        context: &'a AuthContext,
    ) -> ClientOperation<'a, ClientAuthStatus> {
        Box::pin(async move {
            let program = match context.detection_context().executable("claude") {
                Ok(Some(program)) => program,
                Ok(None) => return Ok(unknown("claude executable not found")),
                Err(error) => return Ok(unknown(error)),
            };
            let command = tokio::process::Command::new(program)
                .args(["auth", "status", "--json"])
                .env_clear()
                .envs(context.vars.iter())
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .output();
            let output = match tokio::time::timeout(self.status_timeout, command).await {
                Err(_) => return Ok(unknown("claude auth status timed out")),
                Ok(Err(error)) => {
                    return Ok(unknown(format!(
                        "claude auth status could not be started: {error}"
                    )));
                }
                Ok(Ok(output)) => output,
            };
            if !output.status.success() {
                return Ok(unknown(format!(
                    "claude auth status exited with {}",
                    output.status
                )));
            }
            Ok(parse_status(&output.stdout))
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
        Box::pin(async {
            Err(Error::CapabilityUnavailable(
                "Pontia does not manage Claude Code login".into(),
            ))
        })
    }

    fn logout<'a>(
        &'a self,
        _key: &'a str,
        _scope: Option<&'a str>,
        _context: &'a AuthContext,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async {
            Err(Error::CapabilityUnavailable(
                "Pontia does not manage Claude Code logout".into(),
            ))
        })
    }
}

#[cfg(test)]
mod tests;

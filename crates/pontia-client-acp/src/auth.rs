//! Subscription login for the built-in Kiro and Grok ACP agents.

mod device;
mod grok;
mod kiro;

use std::{collections::HashMap, path::PathBuf, process::Output, sync::Arc, time::Duration};

use pontia_agent_control::{
    client_auth::sanitize_output,
    client_contract::{
        ClientAuth, ClientOperation, DetectionContext,
        auth::{
            AuthContext, AuthEventSink, AuthFlowHandle, AuthMethodDescriptor, AuthState,
            ClientAuthStatus, StartLoginParams,
        },
    },
};
use pontia_core::{Error, Result};

use crate::configuration::AcpConfiguration;

const STATUS_TIMEOUT: Duration = Duration::from_secs(5);
const LOGOUT_TIMEOUT: Duration = Duration::from_secs(10);
const DEVICE_CODE_TIMEOUT: Duration = Duration::from_secs(15);

pub(crate) struct AcpAuth {
    config: Arc<AcpConfiguration>,
}

impl AcpAuth {
    pub(crate) fn new(config: Arc<AcpConfiguration>) -> Self {
        Self { config }
    }

    fn cli(&self, key: &str, context: &AuthContext) -> Result<Cli> {
        let launch = &self.config.definition(key)?.launch;
        let mut vars = (*context.vars).clone();
        vars.extend(launch.env.clone());
        let program = DetectionContext {
            vars: &vars,
            pontia_home: &context.pontia_home,
        }
        .executable(&launch.command)
        .map_err(Error::CapabilityUnavailable)?
        .ok_or_else(|| {
            Error::CapabilityUnavailable(format!("{} executable not found", launch.command))
        })?;
        Ok(Cli { program, vars })
    }
}

struct Cli {
    program: PathBuf,
    vars: HashMap<String, String>,
}

impl Cli {
    async fn run(&self, args: &[&str], limit: Duration) -> Result<Output> {
        let command = tokio::process::Command::new(&self.program)
            .args(args)
            .env_clear()
            .envs(&self.vars)
            .env("NO_COLOR", "1")
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true)
            .output();
        tokio::time::timeout(limit, command)
            .await
            .map_err(|_| Error::CapabilityUnavailable(format!("{} timed out", args.join(" "))))?
            .map_err(Error::from)
    }

    fn device_login(self, args: Vec<String>, expires_in_secs: Option<u64>) -> device::DeviceLogin {
        device::DeviceLogin {
            program: self.program,
            args,
            vars: self.vars,
            expires_in_secs,
            parse_timeout: DEVICE_CODE_TIMEOUT,
        }
    }

    /// Already signed out counts as success.
    async fn logout(&self, signed_out: &[&str]) -> Result<()> {
        let output = self.run(&["logout"], LOGOUT_TIMEOUT).await?;
        let text = combined_output(&output);
        if output.status.success()
            || signed_out
                .iter()
                .any(|marker| text.to_ascii_lowercase().contains(marker))
        {
            return Ok(());
        }
        Err(Error::Domain(format!(
            "Logout failed: {}",
            sanitize_output(&text)
        )))
    }
}

fn combined_output(output: &Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn unknown(methods: Vec<AuthMethodDescriptor>, error: &Error) -> ClientAuthStatus {
    let detail = match error {
        Error::CapabilityUnavailable(message) | Error::Domain(message) => message.clone(),
        other => other.to_string(),
    };
    ClientAuthStatus::new(AuthState::Unknown, methods, true).with_detail(sanitize_output(&detail))
}

fn unsupported(key: &str) -> Error {
    Error::CapabilityUnavailable(format!(
        "Pontia does not manage authentication for ACP Agent {key}"
    ))
}

impl ClientAuth for AcpAuth {
    fn supports_auth(&self, key: &str) -> bool {
        matches!(key, "kiro" | "grok") && self.config.agents.get(key).is_some()
    }

    fn status<'a>(
        &'a self,
        key: &'a str,
        context: &'a AuthContext,
    ) -> ClientOperation<'a, ClientAuthStatus> {
        Box::pin(async move {
            let methods = match key {
                "kiro" => kiro::methods(),
                "grok" => grok::methods(),
                _ => {
                    return Ok(ClientAuthStatus::new(
                        AuthState::Unsupported,
                        Vec::new(),
                        false,
                    ));
                }
            };
            let cli = match self.cli(key, context) {
                Ok(cli) => cli,
                Err(error) => return Ok(unknown(methods, &error)),
            };
            Ok(match key {
                "kiro" => match cli
                    .run(&["whoami", "--format", "json"], STATUS_TIMEOUT)
                    .await
                {
                    Ok(output) => kiro::status(&output),
                    Err(error) => unknown(methods, &error),
                },
                _ => grok::status(&cli.vars),
            })
        })
    }

    fn start_login<'a>(
        &'a self,
        key: &'a str,
        method_id: &'a str,
        params: StartLoginParams,
        context: &'a AuthContext,
        sink: AuthEventSink,
    ) -> ClientOperation<'a, AuthFlowHandle> {
        Box::pin(async move {
            let (args, expires_in_secs) = match key {
                "kiro" => (
                    kiro::login_args(method_id, &params)?,
                    Some(kiro::DEVICE_CODE_LIFETIME_SECS),
                ),
                "grok" => (grok::login_args(method_id, &params)?, None),
                _ => return Err(unsupported(key)),
            };
            let login = self.cli(key, context)?.device_login(args, expires_in_secs);
            device::start(login, sink)
        })
    }

    fn logout<'a>(
        &'a self,
        key: &'a str,
        _scope: Option<&'a str>,
        context: &'a AuthContext,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            let signed_out: &[&str] = match key {
                "kiro" => &["not logged in"],
                "grok" => &["no cached session"],
                _ => return Err(unsupported(key)),
            };
            self.cli(key, context)?.logout(signed_out).await
        })
    }
}

#[cfg(test)]
mod tests;

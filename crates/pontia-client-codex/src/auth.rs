//! ChatGPT subscription login through the Codex app-server account API.

use std::{sync::Arc, time::Duration};

use pontia_agent_control::{
    client_auth::sanitize_output,
    client_contract::{
        ClientAuth, ClientOperation,
        auth::{
            AuthAccountInfo, AuthContext, AuthErrorCode, AuthEventSink, AuthFlowHandle,
            AuthFlowUpdate, AuthMethodDescriptor, AuthMethodKind, AuthSessionState, AuthState,
            AuthStep, ClientAuthStatus, StartLoginParams,
        },
    },
};
use pontia_core::{Error, Result};
use serde_json::{Value, json};
use tokio::sync::broadcast::error::RecvError;

use crate::runtime::{CodexRuntime, CodexServerTarget, protocol::Connection};

const DEVICE_CODE: &str = "chatgpt_device_code";
const DEVICE_CODE_LIFETIME_SECS: u64 = 900;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct CodexAuth;

fn methods() -> Vec<AuthMethodDescriptor> {
    vec![AuthMethodDescriptor {
        id: DEVICE_CODE.into(),
        label: "ChatGPT Subscription (Device Code)".into(),
        description: Some("Sign in with your ChatGPT account in any browser.".into()),
        kind: AuthMethodKind::DeviceCode,
        recommended: true,
        remote_compatible: true,
        start_fields: Vec::new(),
    }]
}

/// Prefer the user daemon so it updates its account state immediately.
async fn connect(context: &AuthContext) -> Result<Arc<Connection>> {
    let runtime = match CodexRuntime::ensure(&context.pontia_home, CodexServerTarget::User).await {
        Ok(runtime) => runtime,
        Err(user) => {
            if crate::setup::resolve_executable(context.detection_context().search_path()).is_err()
            {
                return Err(user);
            }
            CodexRuntime::ensure(&context.pontia_home, CodexServerTarget::Background).await?
        }
    };
    runtime.connection().await
}

fn status_from_account(response: &Value) -> ClientAuthStatus {
    let mut status = ClientAuthStatus::new(AuthState::Authenticated, methods(), true);
    let account = &response["account"];
    let text = |field: &str| account[field].as_str().map(str::to_owned);
    match account["type"].as_str() {
        None if response["requiresOpenaiAuth"] == false => {
            status.detail = Some("OpenAI authentication not required by current provider".into());
        }
        None => status.state = AuthState::Unauthenticated,
        Some("chatgpt") => {
            status.method_id = Some(DEVICE_CODE.into());
            status.account = Some(AuthAccountInfo {
                label: text("email"),
                plan: text("planType"),
                provider: Some("openai".into()),
                workspace: text("workspaceName"),
                expires_at: None,
            });
        }
        Some("apiKey") => {
            status.method_id = Some("api_key".into());
            status.account = Some(AuthAccountInfo {
                label: Some("API Key".into()),
                provider: Some("openai".into()),
                ..AuthAccountInfo::default()
            });
        }
        Some(other) => {
            status.account = Some(AuthAccountInfo {
                provider: Some(other.into()),
                ..AuthAccountInfo::default()
            });
        }
    }
    status
}

async fn read_account(connection: &Connection) -> Result<Value> {
    tokio::time::timeout(
        READ_TIMEOUT,
        connection.call("account/read", json!({"refreshToken": false})),
    )
    .await
    .map_err(|_| Error::ControlUnknown("Codex account/read timed out".into()))?
}

fn required(response: &Value, field: &str) -> Result<String> {
    response[field]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            Error::CapabilityUnavailable(format!("Codex login response is missing {field}"))
        })
}

impl ClientAuth for CodexAuth {
    fn supports_auth(&self, _key: &str) -> bool {
        true
    }

    fn status<'a>(
        &'a self,
        _key: &'a str,
        context: &'a AuthContext,
    ) -> ClientOperation<'a, ClientAuthStatus> {
        Box::pin(async move {
            let result = async { read_account(connect(context).await?.as_ref()).await }.await;
            Ok(match result {
                Ok(response) => status_from_account(&response),
                Err(error) => ClientAuthStatus::new(AuthState::Unknown, methods(), true)
                    .with_detail(sanitize_output(&error.to_string())),
            })
        })
    }

    fn start_login<'a>(
        &'a self,
        _key: &'a str,
        method_id: &'a str,
        _params: StartLoginParams,
        context: &'a AuthContext,
        sink: AuthEventSink,
    ) -> ClientOperation<'a, AuthFlowHandle> {
        Box::pin(async move {
            if method_id != DEVICE_CODE {
                return Err(Error::Domain(format!(
                    "Unsupported Codex login method: {method_id}"
                )));
            }
            let connection = connect(context).await?;
            // Subscribe first: completion can arrive right after the response.
            let mut events = connection.events.subscribe();
            let response = connection
                .call("account/login/start", json!({"type": "chatgptDeviceCode"}))
                .await?;
            let login = required(&response, "loginId")?;
            let verification_url = required(&response, "verificationUrl")?;
            let user_code = required(&response, "userCode")?;
            sink.emit(AuthFlowUpdate::Step {
                state: AuthSessionState::AwaitingUser,
                step: AuthStep::DeviceCode {
                    verification_url,
                    user_code,
                    expires_in_secs: Some(DEVICE_CODE_LIFETIME_SECS),
                    message: None,
                },
                expires_in_secs: Some(DEVICE_CODE_LIFETIME_SECS),
            })
            .await;
            let watched = connection.clone();
            let expected = login.clone();
            let watcher = tokio::spawn(async move {
                let update = loop {
                    match events.recv().await {
                        Ok(event) => {
                            if let Some(update) = login_update(&event, &expected) {
                                break update;
                            }
                        }
                        Err(RecvError::Lagged(_)) => {
                            if read_account(&watched)
                                .await
                                .is_ok_and(|account| !account["account"].is_null())
                            {
                                break AuthFlowUpdate::Completed;
                            }
                        }
                        Err(RecvError::Closed) => break disconnected(),
                    }
                };
                sink.emit(update).await;
            });
            Ok(AuthFlowHandle {
                input: None,
                cancel: Box::new(move || {
                    Box::pin(async move {
                        watcher.abort();
                        let cancelled = tokio::time::timeout(
                            CANCEL_TIMEOUT,
                            connection.call("account/login/cancel", json!({"loginId": login})),
                        )
                        .await
                        .map_err(|_| {
                            Error::ControlUnknown("Codex login cancel timed out".into())
                        })?;
                        // "canceled" and "notFound" both leave no login in progress.
                        cancelled.map(drop)
                    })
                }),
            })
        })
    }

    fn logout<'a>(
        &'a self,
        _key: &'a str,
        _scope: Option<&'a str>,
        context: &'a AuthContext,
    ) -> ClientOperation<'a, ()> {
        Box::pin(async move {
            connect(context)
                .await?
                .call("account/logout", Value::Null)
                .await
                .map(drop)
        })
    }
}

fn disconnected() -> AuthFlowUpdate {
    AuthFlowUpdate::Failed {
        code: AuthErrorCode::ClientUnavailable,
        message: "Codex app-server disconnected during login".into(),
        retryable: true,
    }
}

fn login_update(event: &Value, login: &str) -> Option<AuthFlowUpdate> {
    match event["method"].as_str()? {
        "account/login/completed" => {
            let params = &event["params"];
            if params["loginId"]
                .as_str()
                .is_some_and(|completed| completed != login)
            {
                return None;
            }
            Some(if params["success"] == true {
                AuthFlowUpdate::Completed
            } else {
                AuthFlowUpdate::Failed {
                    code: AuthErrorCode::ProviderRejected,
                    message: sanitize_output(
                        params["error"].as_str().unwrap_or("Codex login failed"),
                    ),
                    retryable: true,
                }
            })
        }
        "pontia/disconnected" => Some(disconnected()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;

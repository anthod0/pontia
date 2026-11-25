//! Client-neutral authentication contract implemented by each client integration.

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    path::PathBuf,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::{ClientOperation, DetectionContext};

/// The device environment used for status checks and login commands, never the browser's.
#[derive(Clone)]
pub struct AuthContext {
    pub pontia_home: PathBuf,
    pub vars: Arc<HashMap<String, String>>,
}

impl AuthContext {
    pub fn detection_context(&self) -> DetectionContext<'_> {
        DetectionContext {
            vars: &self.vars,
            pontia_home: &self.pontia_home,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthState {
    Authenticated,
    Unauthenticated,
    Expired,
    /// The client is missing, unreachable, or did not answer in time.
    Unknown,
    /// Pontia does not manage authentication for this client entry.
    Unsupported,
}

/// Non-secret account details. Never contains tokens, keys, or cookies.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthAccountInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethodKind {
    DeviceCode,
    OAuthLoopbackRelay,
    OAuthCodePaste,
    ApiKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthFieldOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthFieldKind {
    Text,
    Url,
    Secret,
    Select { options: Vec<AuthFieldOption> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthFieldDescriptor {
    pub name: String,
    pub label: String,
    pub kind: AuthFieldKind,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMethodDescriptor {
    /// Stable within the client entry, such as `chatgpt_device_code`.
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kind: AuthMethodKind,
    pub recommended: bool,
    /// Usable from a remote browser over the E2E session.
    pub remote_compatible: bool,
    #[serde(default)]
    pub start_fields: Vec<AuthFieldDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientAuthStatus {
    pub state: AuthState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AuthAccountInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method_id: Option<String>,
    /// Sanitized supplementary information.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub checked_at: String,
    pub methods: Vec<AuthMethodDescriptor>,
    pub supports_logout: bool,
    /// Filled by the session manager while a login session is in progress.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_session_id: Option<String>,
}

impl ClientAuthStatus {
    /// A status checked now, without account details.
    pub fn new(
        state: AuthState,
        methods: Vec<AuthMethodDescriptor>,
        supports_logout: bool,
    ) -> Self {
        Self {
            state,
            account: None,
            method_id: None,
            detail: None,
            checked_at: rfc3339(pontia_core::time::utc_now()),
            methods,
            supports_logout,
            active_session_id: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

pub(crate) fn rfc3339(time: time::OffsetDateTime) -> String {
    let time = time.to_offset(time::UtcOffset::UTC);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        time.year(),
        u8::from(time.month()),
        time.day(),
        time.hour(),
        time.minute(),
        time.second(),
        time.millisecond()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackRelayMode {
    PasteCallbackUrl,
    PasteAuthCode,
    AutoLocalCallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthErrorCode {
    ClientNotInstalled,
    ClientUnavailable,
    MethodUnsupported,
    InvalidInput,
    CallbackValidationFailed,
    CallbackRelayFailed,
    ProviderRejected,
    ParseFailed,
    Timeout,
    Cancelled,
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthStep {
    Pending {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    DeviceCode {
        verification_url: String,
        user_code: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_in_secs: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    BrowserRedirect {
        auth_url: String,
        relay_mode: CallbackRelayMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        callback_hint: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    AwaitInput {
        prompt: String,
        fields: Vec<AuthFieldDescriptor>,
    },
    Completing {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Succeeded {
        status: Box<ClientAuthStatus>,
    },
    Failed {
        code: AuthErrorCode,
        message: String,
        retryable: bool,
    },
    Cancelled {
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
}

/// Sensitive input submitted during login. `Debug` never prints values.
#[derive(Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthInput {
    CallbackUrl { url: String },
    AuthCode { code: String },
    Secret { value: String },
    Fields { values: BTreeMap<String, String> },
    Confirm,
}

impl fmt::Debug for AuthInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CallbackUrl { .. } => f.write_str("AuthInput::CallbackUrl([REDACTED])"),
            Self::AuthCode { .. } => f.write_str("AuthInput::AuthCode([REDACTED])"),
            Self::Secret { .. } => f.write_str("AuthInput::Secret([REDACTED])"),
            Self::Fields { values } => {
                let keys: Vec<&str> = values.keys().map(String::as_str).collect();
                write!(f, "AuthInput::Fields(keys={keys:?}, values=[REDACTED])")
            }
            Self::Confirm => f.write_str("AuthInput::Confirm"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthSessionState {
    Starting,
    AwaitingUser,
    Completing,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

impl AuthSessionState {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::TimedOut
        )
    }
}

/// One login session as returned by REST and pushed over SSE.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessionSnapshot {
    pub session_id: String,
    pub client_type: String,
    pub client_key: String,
    pub method_id: String,
    pub state: AuthSessionState,
    pub step: AuthStep,
    pub sequence: u64,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_at: Option<String>,
}

/// Login start parameters declared by `start_fields`. `Debug` never prints values.
#[derive(Clone, Default, Deserialize)]
pub struct StartLoginParams {
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

impl fmt::Debug for StartLoginParams {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let keys: Vec<&str> = self.fields.keys().map(String::as_str).collect();
        write!(f, "StartLoginParams(keys={keys:?}, values=[REDACTED])")
    }
}

/// Progress reported by a client login flow. The session manager refreshes the
/// status and invalidates the model catalog after `Completed`.
#[derive(Debug, Clone)]
pub enum AuthFlowUpdate {
    Step {
        state: AuthSessionState,
        step: AuthStep,
        /// Shortens the session lifetime when the provider grants less time.
        expires_in_secs: Option<u64>,
    },
    /// The client stored its credential.
    Completed,
    Failed {
        code: AuthErrorCode,
        message: String,
        retryable: bool,
    },
    Cancelled {
        message: Option<String>,
    },
}

#[derive(Clone)]
pub struct AuthEventSink {
    tx: mpsc::Sender<AuthFlowUpdate>,
}

impl AuthEventSink {
    pub fn new(tx: mpsc::Sender<AuthFlowUpdate>) -> Self {
        Self { tx }
    }

    /// Returns `false` once the session manager stopped listening.
    pub async fn emit(&self, update: AuthFlowUpdate) -> bool {
        self.tx.send(update).await.is_ok()
    }
}

pub type AuthCancel = Box<dyn FnOnce() -> ClientOperation<'static, ()> + Send>;

/// The running login flow returned by [`ClientAuth::start_login`].
pub struct AuthFlowHandle {
    /// Present when the flow accepts user input.
    pub input: Option<mpsc::Sender<AuthInput>>,
    /// Idempotent and bounded; stops child processes and removes temporary files.
    pub cancel: AuthCancel,
}

/// Authentication operations implemented by a client integration (ADR 0002).
pub trait ClientAuth: Send + Sync {
    /// Whether Pontia manages authentication for this entry. Performs no I/O.
    fn supports_auth(&self, key: &str) -> bool;

    /// Current state and available login methods.
    fn status<'a>(
        &'a self,
        key: &'a str,
        ctx: &'a AuthContext,
    ) -> ClientOperation<'a, ClientAuthStatus>;

    /// Validate parameters, start the native flow, and return promptly. Later
    /// progress is reported through `sink`.
    fn start_login<'a>(
        &'a self,
        key: &'a str,
        method_id: &'a str,
        params: StartLoginParams,
        ctx: &'a AuthContext,
        sink: AuthEventSink,
    ) -> ClientOperation<'a, AuthFlowHandle>;

    /// Remove the client's native credential. `scope` selects one provider where
    /// a client stores several.
    fn logout<'a>(
        &'a self,
        key: &'a str,
        scope: Option<&'a str>,
        ctx: &'a AuthContext,
    ) -> ClientOperation<'a, ()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_prints_sensitive_input() {
        let inputs: Vec<AuthInput> = [
            r#"{"type":"callback_url","url":"http://127.0.0.1:1/cb?code=secret-1"}"#,
            r#"{"type":"auth_code","code":"secret-2"}"#,
            r#"{"type":"secret","value":"secret-3"}"#,
            r#"{"type":"fields","values":{"region":"secret-4"}}"#,
        ]
        .into_iter()
        .map(|json| serde_json::from_str(json).unwrap())
        .collect();
        let printed = format!("{inputs:?}");
        assert!(!printed.contains("secret-"), "{printed}");
        assert!(printed.contains("region"));
        let params = StartLoginParams {
            fields: [("api_key".into(), "secret-5".into())].into(),
        };
        let printed = format!("{params:?}");
        assert!(!printed.contains("secret-5") && printed.contains("api_key"));
    }

    #[test]
    fn auth_input_rejects_unknown_fields() {
        assert!(
            serde_json::from_str::<AuthInput>(r#"{"type":"auth_code","code":"x","extra":1}"#)
                .is_err()
        );
    }

    #[test]
    fn timestamps_use_millisecond_utc_rfc3339() {
        let time =
            time::OffsetDateTime::from_unix_timestamp_nanos(1_795_678_200_123_456_000).unwrap();
        assert_eq!(rfc3339(time), "2026-11-26T07:30:00.123Z");
    }
}

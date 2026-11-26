use std::{collections::HashMap, fs, os::unix::fs::PermissionsExt, sync::Arc, time::Duration};

use pontia_agent_control::client_contract::{ClientAuth, auth::AuthContext};

use super::*;

static COMMAND_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct Fixture {
    root: tempfile::TempDir,
    auth: ClaudeAuth,
}

impl Fixture {
    fn new(script: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("claude");
        fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        Self {
            root,
            auth: ClaudeAuth::default(),
        }
    }

    fn context(&self) -> AuthContext {
        AuthContext {
            pontia_home: self.root.path().join("pontia"),
            vars: Arc::new(HashMap::from([
                (
                    "PATH".into(),
                    self.root.path().as_os_str().to_string_lossy().into_owned(),
                ),
                (
                    "OUT".into(),
                    self.root.path().as_os_str().to_string_lossy().into_owned(),
                ),
            ])),
        }
    }

    async fn status(&self) -> ClientAuthStatus {
        let _guard = COMMAND_LOCK.lock().await;
        self.auth.status("default", &self.context()).await.unwrap()
    }
}

#[test]
fn registration_exposes_status_only_auth() {
    let auth = crate::registration().auth.unwrap();
    assert!(auth.supports_auth("default"));
    assert!(!auth.supports_auth("other"));
}

#[tokio::test]
async fn reports_authenticated_account_from_stdout_json() {
    let fixture = Fixture::new(
        r#"printf '%s' '{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","email":"user@example.com","orgName":"Example Org","subscriptionType":"max"}'"#,
    );
    let status = fixture.status().await;
    assert_eq!(status.state, AuthState::Authenticated);
    assert!(status.methods.is_empty());
    assert!(!status.supports_logout);
    assert_eq!(
        status.account,
        Some(AuthAccountInfo {
            label: Some("user@example.com".into()),
            plan: Some("max".into()),
            provider: Some("firstParty".into()),
            workspace: Some("Example Org".into()),
            expires_at: None,
        })
    );
}

#[tokio::test]
async fn accepts_alternate_optional_account_fields() {
    let fixture = Fixture::new(
        r#"printf '%s' '{"logged_in":true,"authMethod":"apiKey","org":"Example","subscription":"team"}'"#,
    );
    let account = fixture.status().await.account.unwrap();
    assert_eq!(account.label.as_deref(), Some("Example"));
    assert_eq!(account.plan.as_deref(), Some("team"));
    assert_eq!(account.provider.as_deref(), Some("apiKey"));
    assert_eq!(account.workspace.as_deref(), Some("Example"));
}

#[tokio::test]
async fn reports_unauthenticated_from_stdout_json() {
    let fixture = Fixture::new(r#"printf '%s' '{"loggedIn":false}'"#);
    let status = fixture.status().await;
    assert_eq!(status.state, AuthState::Unauthenticated);
    assert!(status.account.is_none());
    assert!(status.methods.is_empty());
    assert!(!status.supports_logout);
}

#[tokio::test]
async fn invalid_json_is_unknown() {
    let fixture = Fixture::new("printf '%s' 'not json'");
    let status = fixture.status().await;
    assert_eq!(status.state, AuthState::Unknown);
    assert!(status.account.is_none());
}

#[tokio::test]
async fn command_failure_is_unknown() {
    let fixture = Fixture::new("exit 7");
    let status = fixture.status().await;
    assert_eq!(status.state, AuthState::Unknown);
    assert!(status.account.is_none());
}

#[tokio::test]
async fn command_timeout_is_unknown() {
    let mut fixture = Fixture::new("exec /bin/sleep 60");
    fixture.auth.status_timeout = Duration::from_millis(20);
    let status = fixture.status().await;
    assert_eq!(status.state, AuthState::Unknown);
    assert!(status.account.is_none());
}

#[tokio::test]
async fn invokes_status_without_stdin_or_a_tty() {
    let fixture = Fixture::new(
        r#"printf '%s\n' "$@" > "$OUT/args"
if [ -t 0 ] || IFS= read -r line; then exit 9; fi
printf '%s' '{"loggedIn":false}'"#,
    );
    let status = fixture.status().await;
    assert_eq!(status.state, AuthState::Unauthenticated);
    assert_eq!(
        fs::read_to_string(fixture.root.path().join("args")).unwrap(),
        "auth\nstatus\n--json\n"
    );
}

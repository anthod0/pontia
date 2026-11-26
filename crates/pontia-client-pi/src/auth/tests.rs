use std::{collections::HashMap, os::unix::fs::PermissionsExt};

use pontia_agent_control::client_contract::auth::AuthState;

use super::*;

struct Fixture {
    root: tempfile::TempDir,
    auth: PiAuth,
}

impl Fixture {
    fn new(models: &str, auth_command: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let program = root.path().join("pi-test");
        let script = format!(
            r#"#!/bin/sh
printf '%s\n' "$*" >> "$OUT/args"
case "$*" in
  *"--mode rpc"*)
    read request
    printf '%s\n' '{{"type":"response","id":"models","success":true,"data":{{"models":{models}}}}}'
    ;;
  *"auth check"*)
    {auth_command}
    ;;
  *)
    exit 64
    ;;
esac
"#
        );
        std::fs::write(&program, script).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let configuration = Arc::new(PiConfiguration::new(
            json!({"tui_command": program}),
            json!({}),
        ));
        Self {
            root,
            auth: PiAuth::new(configuration),
        }
    }

    fn context(&self) -> AuthContext {
        AuthContext {
            pontia_home: self.root.path().to_path_buf(),
            vars: Arc::new(HashMap::from([
                ("OUT".into(), self.root.path().display().to_string()),
                ("PATH".into(), "/usr/bin:/bin".into()),
            ])),
        }
    }

    fn args(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.path().join("args"))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

#[test]
fn registration_exposes_status_only_authentication() {
    let registration = crate::registration(None);
    let auth = registration.auth.unwrap();
    assert!(auth.supports_auth("default"));
    assert!(!auth.supports_auth("other"));
}

#[tokio::test]
async fn no_available_models_is_unauthenticated() {
    let fixture = Fixture::new("[]", "exit 64");
    let result = fixture
        .auth
        .status("default", &fixture.context())
        .await
        .unwrap();
    assert_eq!(result.state, AuthState::Unauthenticated);
    assert!(result.methods.is_empty());
    assert!(!result.supports_logout);
    let args = fixture.args();
    assert_eq!(args.len(), 1);
    assert!(args[0].contains("--offline") && args[0].contains("--mode rpc"));
}

#[tokio::test]
async fn ready_providers_are_authenticated_without_requesting_credentials() {
    let fixture = Fixture::new(
        r#"[{"provider":"zeta"},{"provider":"alpha"},{"provider":"zeta"}]"#,
        r#"
case "$*" in
  *"--provider alpha"*) printf '%s\n' '{"status":"ready","provider":"alpha","authType":"oauth"}';;
  *"--provider zeta"*) printf '%s\n' '{"status":"ready","provider":"zeta","authType":"api_key"}';;
esac
"#,
    );
    let result = fixture
        .auth
        .status("default", &fixture.context())
        .await
        .unwrap();
    assert_eq!(result.state, AuthState::Authenticated);
    let account = result.account.unwrap();
    assert_eq!(account.label.as_deref(), Some("alpha"));
    assert_eq!(account.provider.as_deref(), Some("alpha"));
    assert_eq!(account.plan.as_deref(), Some("oauth"));
    assert_eq!(
        result.detail.as_deref(),
        Some("Ready providers: alpha (oauth), zeta (api_key)")
    );
    let args = fixture.args();
    assert_eq!(args.len(), 3);
    for args in args.iter().filter(|args| args.contains("auth check")) {
        assert!(args.contains("--json") && args.contains("--no-refresh"));
        assert!(!args.contains("--credentials"));
    }
}

#[tokio::test]
async fn providers_without_credentials_are_unauthenticated() {
    let fixture = Fixture::new(
        r#"[{"provider":"openai"}]"#,
        r#"printf '%s\n' '{"status":"not_ready","provider":"openai","reason":"credentials_not_configured"}'; exit 1"#,
    );
    let result = fixture
        .auth
        .status("default", &fixture.context())
        .await
        .unwrap();
    assert_eq!(result.state, AuthState::Unauthenticated);
    assert!(result.account.is_none());
}

#[tokio::test]
async fn available_extension_provider_is_authenticated_when_auth_check_cannot_load_it() {
    let fixture = Fixture::new(
        r#"[{"provider":"kiro"}]"#,
        r#"printf '%s\n' '{"status":"not_ready","provider":"kiro","reason":"provider_not_found"}'; exit 1"#,
    );
    let result = fixture
        .auth
        .status("default", &fixture.context())
        .await
        .unwrap();
    assert_eq!(result.state, AuthState::Authenticated);
    let account = result.account.unwrap();
    assert_eq!(account.provider.as_deref(), Some("kiro"));
    assert!(account.plan.is_none());
}

#[tokio::test]
async fn invalid_or_unparseable_checks_are_unknown() {
    for auth_command in [
        r#"printf '%s\n' '{"status":"invalid","provider":"openai","reason":"invalid_state"}'; exit 2"#,
        r#"printf '%s\n' 'not json'; exit 2"#,
        "exit 127",
    ] {
        let fixture = Fixture::new(r#"[{"provider":"openai"}]"#, auth_command);
        let result = fixture
            .auth
            .status("default", &fixture.context())
            .await
            .unwrap();
        assert_eq!(result.state, AuthState::Unknown);
        assert!(result.detail.is_some());
    }
}

#[tokio::test]
async fn status_probe_times_out() {
    let fixture = Fixture::new(r#"[{"provider":"openai"}]"#, "sleep 2");
    let result = fixture
        .auth
        .status("default", &fixture.context())
        .await
        .unwrap();
    assert_eq!(result.state, AuthState::Unknown);
    assert_eq!(
        result.detail.as_deref(),
        Some("Pi authentication status check timed out")
    );
}

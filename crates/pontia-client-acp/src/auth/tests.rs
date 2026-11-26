use std::{os::unix::process::ExitStatusExt, path::Path, time::Duration};

use pontia_agent_control::client_contract::auth::{
    AuthErrorCode, AuthFlowUpdate, AuthSessionState, AuthStep,
};
use pontia_runtime::{ProcessFingerprint, ProcessObservation};
use serde_json::json;
use tokio::sync::mpsc;

use super::*;
use crate::AcpAgents;

struct Fixture {
    directory: tempfile::TempDir,
    auth: AcpAuth,
}

impl Fixture {
    /// Every built-in agent runs `script`.
    fn new(script: &str) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let program = directory.path().join("client");
        std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let builtin = AcpAgents::builtin();
        let agents = AcpAgents::new(builtin.ids().map(|id| {
            let mut agent = builtin.get(id).unwrap().clone();
            agent.launch.command = program.display().to_string();
            (id.to_owned(), agent)
        }));
        Self {
            directory,
            auth: AcpAuth::new(Arc::new(AcpConfiguration::new(agents))),
        }
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.directory.path().join(name)
    }

    fn context(&self, extra: &[(&str, &str)]) -> AuthContext {
        let mut vars = HashMap::from([
            ("PATH".to_owned(), std::env::var("PATH").unwrap()),
            (
                "OUT".to_owned(),
                self.directory.path().display().to_string(),
            ),
            (
                "HOME".to_owned(),
                self.directory.path().display().to_string(),
            ),
        ]);
        vars.extend(
            extra
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
        );
        AuthContext {
            pontia_home: self.directory.path().to_path_buf(),
            vars: Arc::new(vars),
        }
    }

    fn args(&self) -> String {
        std::fs::read_to_string(self.path("args"))
            .unwrap()
            .trim()
            .to_owned()
    }

    async fn login(
        &self,
        key: &str,
        method: &str,
        fields: &[(&str, &str)],
    ) -> (Result<AuthFlowHandle>, mpsc::Receiver<AuthFlowUpdate>) {
        let (sender, updates) = mpsc::channel(8);
        let params = StartLoginParams {
            fields: fields
                .iter()
                .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                .collect(),
        };
        let handle = self
            .auth
            .start_login(
                key,
                method,
                params,
                &self.context(&[]),
                AuthEventSink::new(sender),
            )
            .await;
        (handle, updates)
    }
}

const RECORD_ARGS: &str = r#"echo "$@" > "$OUT/args""#;

async fn next(updates: &mut mpsc::Receiver<AuthFlowUpdate>) -> AuthFlowUpdate {
    tokio::time::timeout(Duration::from_secs(10), updates.recv())
        .await
        .expect("login update")
        .expect("open login flow")
}

fn device_code(update: AuthFlowUpdate) -> (String, String, Option<u64>) {
    let AuthFlowUpdate::Step {
        state: AuthSessionState::AwaitingUser,
        step:
            AuthStep::DeviceCode {
                verification_url,
                user_code,
                expires_in_secs,
                ..
            },
        ..
    } = update
    else {
        panic!("expected a device code, got {update:?}");
    };
    (verification_url, user_code, expires_in_secs)
}

fn output(code: i32, stdout: &str, stderr: &str) -> Output {
    Output {
        status: std::process::ExitStatus::from_raw(code << 8),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
    }
}

#[test]
fn built_in_auth_entries_are_supported() {
    let fixture = Fixture::new("exit 0");
    assert!(fixture.auth.supports_auth("kiro"));
    assert!(fixture.auth.supports_auth("grok"));
    assert!(fixture.auth.supports_auth("antigravity"));
    assert!(!fixture.auth.supports_auth("custom"));
}

#[test]
fn kiro_whoami_reads_the_first_json_object() {
    let status = kiro::status(&output(
        0,
        "{\"accountType\":\"IamIdentityCenter\",\"startUrl\":\"https://d-1.awsapps.com/start\",\"region\":\"us-east-1\",\"email\":\"user@example.com\"}\n\nProfile:\nKiroProfile-us-east-1\narn:aws:codewhisperer:us-east-1:1:profile/X\n",
        "",
    ));
    assert_eq!(status.state, AuthState::Authenticated);
    assert_eq!(status.method_id.as_deref(), Some("kiro_device_pro"));
    let account = status.account.unwrap();
    assert_eq!(account.label.as_deref(), Some("user@example.com"));
    assert_eq!(account.plan.as_deref(), Some("IamIdentityCenter"));
    assert_eq!(account.provider.as_deref(), Some("aws"));
    assert_eq!(
        account.workspace.as_deref(),
        Some("https://d-1.awsapps.com/start")
    );
    let free = kiro::status(&output(0, "{\"accountType\":\"BuilderId\"}\n", ""));
    assert_eq!(free.method_id.as_deref(), Some("kiro_device_free"));
    assert_eq!(free.account.unwrap().label.as_deref(), Some("BuilderId"));
    assert_eq!(status.methods.len(), 2);
}

#[test]
fn kiro_whoami_failures_map_to_states() {
    let signed_out = kiro::status(&output(1, "{\"account\":null}\n", ""));
    assert_eq!(signed_out.state, AuthState::Unauthenticated);
    let plain = kiro::status(&output(1, "Not logged in\n", ""));
    assert_eq!(plain.state, AuthState::Unauthenticated);
    let expired = kiro::status(&output(1, "", "error: session expired, sign in again\n"));
    assert_eq!(expired.state, AuthState::Expired);
    let unknown = kiro::status(&output(2, "", "panic Bearer abc.def\n"));
    assert_eq!(unknown.state, AuthState::Unknown);
    let detail = unknown.detail.unwrap();
    assert!(
        detail.contains("Bearer [REDACTED]") && !detail.contains("abc.def"),
        "{detail}"
    );
}

#[test]
fn kiro_login_parameters_are_validated() {
    let params = |fields: &[(&str, &str)]| StartLoginParams {
        fields: fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
    };
    assert_eq!(
        kiro::login_args("kiro_device_free", &params(&[])).unwrap(),
        ["login", "--license", "free", "--use-device-flow"]
    );
    assert_eq!(
        kiro::login_args(
            "kiro_device_pro",
            &params(&[
                (
                    "identity_provider",
                    "https://d-1234567890.awsapps.com/start"
                ),
                ("region", "eu-west-1"),
            ])
        )
        .unwrap(),
        [
            "login",
            "--license",
            "pro",
            "--identity-provider",
            "https://d-1234567890.awsapps.com/start",
            "--region",
            "eu-west-1",
            "--use-device-flow"
        ]
    );
    for (identity_provider, region) in [
        ("http://d-1.awsapps.com/start", "us-east-1"),
        ("-https://d-1.awsapps.com/start", "us-east-1"),
        ("https://d-1.awsapps.com/start x", "us-east-1"),
        ("https://user@evil/start", "us-east-1"),
        ("https://", "us-east-1"),
        ("https://d-1.awsapps.com/start", "US-EAST-1"),
        ("https://d-1.awsapps.com/start", "us"),
        ("https://d-1.awsapps.com/start", "us-east-1;rm"),
        ("https://d-1.awsapps.com/start", "--region"),
        ("https://d-1.awsapps.com/start", ""),
    ] {
        assert!(
            kiro::login_args(
                "kiro_device_pro",
                &params(&[("identity_provider", identity_provider), ("region", region)])
            )
            .is_err(),
            "{identity_provider} {region}"
        );
    }
    assert!(kiro::login_args("kiro_device_pro", &params(&[("region", "us-east-1")])).is_err());
    assert!(kiro::login_args("kiro_device_free", &params(&[("region", "us-east-1")])).is_err());
    assert!(kiro::login_args("kiro_social", &params(&[])).is_err());
}

#[tokio::test]
async fn kiro_status_runs_whoami_as_json() {
    let fixture = Fixture::new(&format!(
        "{RECORD_ARGS}\nprintf '%s\\n\\nProfile:\\nX\\n' '{{\"accountType\":\"BuilderId\",\"email\":\"a@b.c\"}}'"
    ));
    let status = fixture
        .auth
        .status("kiro", &fixture.context(&[]))
        .await
        .unwrap();
    assert_eq!(fixture.args(), "whoami --format json");
    assert_eq!(status.state, AuthState::Authenticated);
    let missing = Fixture::new("exit 0");
    std::fs::remove_file(missing.path("client")).unwrap();
    let status = missing
        .auth
        .status("kiro", &missing.context(&[]))
        .await
        .unwrap();
    assert_eq!(status.state, AuthState::Unknown);
    assert!(status.detail.unwrap().contains("executable not found"));
}

#[tokio::test]
async fn antigravity_status_is_unknown_without_running_the_client() {
    let fixture = Fixture::new(RECORD_ARGS);
    let status = fixture
        .auth
        .status("antigravity", &fixture.context(&[]))
        .await
        .unwrap();
    assert_eq!(status.state, AuthState::Unknown);
    assert!(status.methods.is_empty());
    assert!(!status.supports_logout);
    assert!(status.detail.is_some());
    assert!(!fixture.path("args").exists());
}

#[tokio::test]
async fn kiro_device_login_reports_the_code_and_cancel_stops_the_process_group() {
    let fixture = Fixture::new(&format!(
        r#"{RECORD_ARGS}
env > "$OUT/env"
printf '\nConfirm the following code in the browser\nCode: \033[1mCLFN-FPLB\033[0m\n\nOpen this URL: https://view.awsapps.com/start/#/device?user_code=CLFN-FPLB\n'
printf '\033[?25l\r Logging in...\r Logging in...'
sleep 60 &
echo $! > "$OUT/grandchild"
wait"#
    ));
    let (handle, mut updates) = fixture.login("kiro", "kiro_device_free", &[]).await;
    let handle = handle.unwrap();
    let (url, code, expires) = device_code(next(&mut updates).await);
    assert_eq!(
        url,
        "https://view.awsapps.com/start/#/device?user_code=CLFN-FPLB"
    );
    assert_eq!(code, "CLFN-FPLB");
    assert_eq!(expires, Some(600));
    assert_eq!(fixture.args(), "login --license free --use-device-flow");
    let environment = std::fs::read_to_string(fixture.path("env")).unwrap();
    for variable in ["NO_COLOR=1", "TERM=dumb", "BROWSER=/bin/false"] {
        assert!(
            environment.lines().any(|line| line == variable),
            "{variable}"
        );
    }
    let grandchild = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(pid) = std::fs::read_to_string(fixture.path("grandchild"))
                && let Ok(pid) = pid.trim().parse::<u32>()
            {
                return ProcessFingerprint::capture(pid).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    (handle.cancel)().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        while grandchild.observe() != ProcessObservation::Exited {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("grandchild exits with the login process group");
}

#[tokio::test]
async fn device_login_completes_or_fails_with_the_exit_status() {
    let fixture = Fixture::new(&format!(
        r#"{RECORD_ARGS}
echo 'To sign in, open this URL in your browser:' >&2
echo '  https://accounts.x.ai/oauth2/device?user_code=4BJN-SZG3' >&2
echo 'Waiting for authorization...' >&2
while [ ! -f "$OUT/authorized" ]; do sleep 0.02; done
rm "$OUT/authorized"
[ -f "$OUT/fail" ] || exit 0
echo 'error: access_denied ?code=leaked' >&2
exit 3"#
    ));
    let (handle, mut updates) = fixture.login("grok", "grok_device_auth", &[]).await;
    let _handle = handle.unwrap();
    let (url, code, expires) = device_code(next(&mut updates).await);
    assert_eq!(
        url,
        "https://accounts.x.ai/oauth2/device?user_code=4BJN-SZG3"
    );
    assert_eq!(code, "4BJN-SZG3");
    assert_eq!(expires, None);
    std::fs::write(fixture.path("authorized"), "").unwrap();
    assert!(matches!(
        next(&mut updates).await,
        AuthFlowUpdate::Completed
    ));
    assert_eq!(fixture.args(), "login --device-auth");

    std::fs::write(fixture.path("fail"), "").unwrap();
    let (handle, mut updates) = fixture.login("grok", "grok_device_auth", &[]).await;
    let _handle = handle.unwrap();
    device_code(next(&mut updates).await);
    std::fs::write(fixture.path("authorized"), "").unwrap();
    let AuthFlowUpdate::Failed {
        code: AuthErrorCode::ProviderRejected,
        message,
        retryable: true,
    } = next(&mut updates).await
    else {
        panic!("expected failure");
    };
    assert!(
        message.contains("access_denied ?code=[REDACTED]"),
        "{message}"
    );
    assert!(!message.contains("leaked"));
}

#[tokio::test]
async fn login_exiting_before_a_code_completes_when_already_signed_in() {
    let fixture = Fixture::new("echo 'Already logged in'");
    let (handle, mut updates) = fixture.login("kiro", "kiro_device_free", &[]).await;
    let _handle = handle.unwrap();
    assert!(matches!(
        next(&mut updates).await,
        AuthFlowUpdate::Completed
    ));
}

#[tokio::test]
async fn login_without_a_device_code_fails_to_parse() {
    let fixture = Fixture::new("echo 'Opening browser'; sleep 60");
    let (sender, mut updates) = mpsc::channel(8);
    let _handle = device::start(
        device::DeviceLogin {
            program: fixture.path("client"),
            args: Vec::new(),
            vars: (*fixture.context(&[]).vars).clone(),
            expires_in_secs: None,
            parse_timeout: Duration::from_millis(200),
        },
        AuthEventSink::new(sender),
    )
    .unwrap();
    assert!(matches!(
        next(&mut updates).await,
        AuthFlowUpdate::Failed {
            code: AuthErrorCode::ParseFailed,
            ..
        }
    ));
}

#[tokio::test]
async fn invalid_logins_are_rejected_before_starting() {
    let fixture = Fixture::new(RECORD_ARGS);
    for (key, method) in [
        ("kiro", "grok_device_auth"),
        ("grok", "kiro_device_free"),
        ("antigravity", "x"),
    ] {
        let (handle, _updates) = fixture.login(key, method, &[]).await;
        assert!(handle.is_err(), "{key} {method}");
    }
    let (handle, _updates) = fixture
        .login(
            "kiro",
            "kiro_device_pro",
            &[
                ("identity_provider", "https://x/start"),
                ("region", "us-east-1; reboot"),
            ],
        )
        .await;
    assert!(matches!(handle, Err(Error::Domain(_))));
    assert!(!fixture.path("args").exists());
}

fn grok_vars(home: &Path, extra: &[(&str, &str)]) -> HashMap<String, String> {
    let mut vars = HashMap::from([("HOME".to_owned(), home.display().to_string())]);
    vars.extend(
        extra
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned())),
    );
    vars
}

fn write_grok_credentials(directory: &Path, entry: serde_json::Value) {
    std::fs::create_dir_all(directory).unwrap();
    std::fs::write(
        directory.join("auth.json"),
        json!({"https://auth.x.ai::user": entry}).to_string(),
    )
    .unwrap();
}

#[test]
fn grok_status_reads_credential_metadata() {
    let home = tempfile::tempdir().unwrap();
    let grok = home.path().join(".grok");
    let missing = grok::status(&grok_vars(home.path(), &[]));
    assert_eq!(missing.state, AuthState::Unauthenticated);

    write_grok_credentials(
        &grok,
        json!({"key":"secret-key","auth_mode":"oidc","email":"user@example.com","team_id":"team","refresh_token":"","expires_at":"2999-01-01T00:00:00Z"}),
    );
    let valid = grok::status(&grok_vars(home.path(), &[]));
    assert_eq!(valid.state, AuthState::Authenticated);
    assert_eq!(valid.method_id.as_deref(), Some("grok_device_auth"));
    let account = valid.account.clone().unwrap();
    assert_eq!(account.label.as_deref(), Some("user@example.com"));
    assert_eq!(account.plan.as_deref(), Some("oidc"));
    assert_eq!(account.provider.as_deref(), Some("xai"));
    assert_eq!(account.workspace.as_deref(), Some("team"));
    assert_eq!(account.expires_at.as_deref(), Some("2999-01-01T00:00:00Z"));
    assert!(
        !serde_json::to_string(&valid)
            .unwrap()
            .contains("secret-key")
    );

    write_grok_credentials(
        &grok,
        json!({"key":"secret-key","expires_at":"2000-01-01T00:00:00Z"}),
    );
    assert_eq!(
        grok::status(&grok_vars(home.path(), &[])).state,
        AuthState::Expired
    );
    write_grok_credentials(
        &grok,
        json!({"key":"","refresh_token":"refresh","expires_at":"2000-01-01T00:00:00Z"}),
    );
    assert_eq!(
        grok::status(&grok_vars(home.path(), &[])).state,
        AuthState::Authenticated
    );

    write_grok_credentials(&grok, json!({"email":"user@example.com"}));
    let environment = grok::status(&grok_vars(home.path(), &[("XAI_API_KEY", "xai-secret")]));
    assert_eq!(environment.state, AuthState::Authenticated);
    assert_eq!(environment.method_id.as_deref(), Some("env_api_key"));
    assert!(
        !serde_json::to_string(&environment)
            .unwrap()
            .contains("xai-secret")
    );

    std::fs::write(grok.join("auth.json"), "not json").unwrap();
    assert_eq!(
        grok::status(&grok_vars(home.path(), &[])).state,
        AuthState::Unknown
    );

    let custom = home.path().join("custom");
    write_grok_credentials(&custom, json!({"key":"k"}));
    let custom_home = custom.display().to_string();
    assert_eq!(
        grok::status(&grok_vars(home.path(), &[("GROK_HOME", &custom_home)])).state,
        AuthState::Authenticated
    );
    assert_eq!(grok::status(&HashMap::new()).state, AuthState::Unknown);
}

#[tokio::test]
async fn logout_accepts_already_signed_out_clients() {
    let fixture = Fixture::new(&format!(
        r#"{RECORD_ARGS}
if [ -f "$OUT/fail" ]; then echo 'boom token=leaked' >&2; exit 1; fi
echo 'Not logged in'; echo 'No cached session to log out of.'; exit 1"#
    ));
    for key in ["kiro", "grok"] {
        fixture
            .auth
            .logout(key, None, &fixture.context(&[]))
            .await
            .unwrap();
        assert_eq!(fixture.args(), "logout");
    }
    std::fs::write(fixture.path("fail"), "").unwrap();
    let Err(Error::Domain(message)) = fixture
        .auth
        .logout("kiro", None, &fixture.context(&[]))
        .await
    else {
        panic!("expected logout failure");
    };
    assert!(message.contains("token=[REDACTED]") && !message.contains("leaked"));
    assert!(
        fixture
            .auth
            .logout("antigravity", None, &fixture.context(&[]))
            .await
            .is_err()
    );
}

use std::{cell::RefCell, collections::HashMap, fs, io::Cursor, net::SocketAddr, path::Path};

use pontia::init::{InitPlatform, run};
use pontia_application::client_contract::{ClientIntegration, PreparedClientIntegration};
use pontia_config::AppConfig;
use pontia_runtime::local_service::{CommandOutput, CommandRunner, DefinitionStore};
use std::{path::PathBuf, sync::Arc};

struct FakeIntegration {
    name: &'static str,
    default: bool,
}
struct FakeSetup {
    name: &'static str,
    home: PathBuf,
}
impl ClientIntegration for FakeIntegration {
    fn client_type(&self) -> &'static str {
        self.name
    }
    fn selected_by_default(&self) -> bool {
        self.default
    }
    fn skipped_summary(&self) -> Vec<String> {
        Vec::new()
    }
    fn prepare(
        &self,
        _vars: &HashMap<String, String>,
        home: &Path,
        runner: &dyn CommandRunner,
    ) -> Result<Box<dyn PreparedClientIntegration>, String> {
        if self.name == "codex" {
            runner.run("inspect-codex", &[])?;
        }
        Ok(Box::new(FakeSetup {
            name: self.name,
            home: home.join(".codex"),
        }))
    }
}
impl PreparedClientIntegration for FakeSetup {
    fn summary(&self) -> Vec<String> {
        Vec::new()
    }
    fn preflight(&self, _runner: &dyn CommandRunner) -> Result<(), String> {
        Ok(())
    }
    fn install(
        &self,
        runner: &dyn CommandRunner,
        _definitions: &dyn DefinitionStore,
    ) -> Result<(), String> {
        let event = match self.name {
            "pi" => "install-pi".to_string(),
            _ => format!("initialize-codex:{}", self.home.display()),
        };
        runner.run(&event, &[]).map(|_| ())
    }
    fn completion(&self) -> &'static str {
        "Installed integration"
    }
    fn service_environment_paths(&self) -> Vec<(String, PathBuf)> {
        if self.name == "codex" {
            vec![("CODEX_HOME".into(), self.home.clone())]
        } else {
            Vec::new()
        }
    }
}

impl CommandRunner for FakePlatform {
    fn run(&self, program: &str, _args: &[String]) -> Result<CommandOutput, String> {
        self.events.borrow_mut().push(program.into());
        if program == "install-pi"
            && let Some(error) = self.install_error
        {
            return Err(error.into());
        }
        Ok(CommandOutput {
            code: 0,
            stdout: String::new(),
            stderr: String::new(),
        })
    }
}
impl DefinitionStore for FakePlatform {
    fn read(&self, _path: &Path) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn install(&self, _path: &Path, _contents: &str) -> Result<bool, String> {
        Ok(true)
    }
}

struct FakePlatform {
    events: RefCell<Vec<String>>,
    install_error: Option<&'static str>,
    dashboard_ready: bool,
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            install_error: None,
            dashboard_ready: true,
        }
    }
}

impl InitPlatform for FakePlatform {
    fn integrations(&self) -> Vec<Arc<dyn ClientIntegration>> {
        vec![
            Arc::new(FakeIntegration {
                name: "pi",
                default: true,
            }),
            Arc::new(FakeIntegration {
                name: "codex",
                default: false,
            }),
        ]
    }
    fn command_runner(&self) -> &dyn CommandRunner {
        self
    }
    fn definition_store(&self) -> &dyn DefinitionStore {
        self
    }
    fn preflight(&self) -> Result<(), String> {
        self.events.borrow_mut().push("preflight".into());
        Ok(())
    }

    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(7);
        Ok(())
    }

    fn start_service(
        &self,
        config: &AppConfig,
        config_changed: bool,
        environment_paths: &[(String, PathBuf)],
    ) -> Result<(), String> {
        self.events.borrow_mut().push(format!(
            "start:{}:{config_changed}:{}",
            config.pontia_home.display(),
            environment_paths
                .first()
                .map(|(_, home)| home.display().to_string())
                .unwrap_or_else(|| "-".to_string())
        ));
        Ok(())
    }

    fn dashboard_available(&self, addr: SocketAddr) -> Result<bool, String> {
        self.events
            .borrow_mut()
            .push(format!("dashboard-ready:{addr}"));
        Ok(self.dashboard_ready)
    }
}

fn vars(home: &Path, pontia_home: &Path) -> HashMap<String, String> {
    HashMap::from([
        ("HOME".to_string(), home.display().to_string()),
        ("PONTIA_HOME".to_string(), pontia_home.display().to_string()),
    ])
}

#[test]
fn default_initialization_installs_pi_writes_config_starts_service_and_returns_dashboard() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"\n\n".to_vec());
    let mut output = Vec::new();

    let outcome = run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("initialize Pontia")
    .expect("initialization completed");

    let config_text = fs::read_to_string(pontia_home.join("config.toml")).expect("read config");
    let config: toml::Value = toml::from_str(&config_text).expect("valid TOML");
    let token = config["external_api_token"].as_str().expect("token");
    assert_eq!(token.len(), 43);
    assert_eq!(config["bind_addr"].as_str(), Some("127.0.0.1:8080"));
    assert_eq!(
        config["workspace_browser"]["roots"][0]["root_id"].as_str(),
        Some("home")
    );
    assert_eq!(
        config["workspace_browser"]["roots"][0]["path"].as_str(),
        Some(user_home.display().to_string().as_str())
    );
    assert_eq!(
        platform.events.borrow().as_slice(),
        [
            "preflight",
            "install-pi",
            &format!("start:{}:true:-", pontia_home.display()),
            "dashboard-ready:127.0.0.1:8080",
        ]
    );
    let expected_url = format!("http://127.0.0.1:8080/dashboard?token={token}");
    assert_eq!(outcome.local_dashboard_url, expected_url);
}

#[test]
fn codex_can_be_initialized_without_pi() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"codex\n\n".to_vec());
    let mut output = Vec::new();

    run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("initialize Codex")
    .expect("initialization completed");

    let events = platform.events.borrow();
    assert!(events.iter().any(|event| event == "inspect-codex"));
    assert!(
        events
            .iter()
            .any(|event| event
                == &format!("initialize-codex:{}", user_home.join(".codex").display()))
    );
    assert!(events.iter().any(|event| event
        == &format!(
            "start:{}:true:{}",
            pontia_home.display(),
            user_home.join(".codex").display()
        )));
}

#[test]
fn codex_can_be_combined_with_pi_during_initialization() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"pi,codex\n\n".to_vec());
    let mut output = Vec::new();

    run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("initialize Pi and Codex")
    .expect("initialization completed");

    let events = platform.events.borrow();
    assert!(events.iter().any(|event| event == "install-pi"));
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("initialize-codex:"))
    );
}

#[test]
fn rerunning_preserves_the_token_comments_and_unknown_config_without_requesting_restart() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    fs::create_dir(&pontia_home).expect("create pontia home");
    fs::write(
        pontia_home.join("config.toml"),
        format!(
            "# keep this comment\nbind_addr = \"127.0.0.1:9090\"\nexternal_api_token = \"existing-token\"\ncustom_setting = \"keep\"\n\n[workspace_browser]\nroots = [\n  # keep this root comment\n  {{ root_id = \"home\", label = \"Home\", path = {:?} }},\n]\n",
            user_home.display().to_string()
        ),
    )
    .expect("write existing config");
    let platform = FakePlatform::default();

    for _ in 0..2 {
        let mut input = Cursor::new(b"\n\n".to_vec());
        let mut output = Vec::new();
        run(
            &mut input,
            &mut output,
            &vars(&user_home, &pontia_home),
            &platform,
        )
        .expect("rerun initialization");
        assert!(
            platform
                .events
                .borrow()
                .contains(&format!("start:{}:false:-", pontia_home.display()))
        );
        platform.events.borrow_mut().clear();
    }

    let config = fs::read_to_string(pontia_home.join("config.toml")).expect("read config");
    assert!(config.contains("# keep this comment"));
    assert!(config.contains("# keep this root comment"));
    assert!(config.contains("custom_setting = \"keep\""));
    assert!(config.contains("external_api_token = \"existing-token\""));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(pontia_home.join("config.toml"))
            .expect("config metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn rerunning_preserves_an_empty_workspace_root_configuration() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    fs::create_dir(&pontia_home).expect("create pontia home");
    fs::write(
        pontia_home.join("config.toml"),
        "[workspace_browser]\nroots = []\n",
    )
    .expect("write existing config");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"\n\n".to_vec());
    let mut output = Vec::new();

    run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("rerun initialization")
    .expect("initialization completed");

    let config_text = fs::read_to_string(pontia_home.join("config.toml")).expect("read config");
    let config: toml::Value = toml::from_str(&config_text).expect("valid TOML");
    assert_eq!(
        config["workspace_browser"]["roots"]
            .as_array()
            .expect("workspace roots")
            .len(),
        0
    );
}

#[test]
fn existing_token_is_query_encoded_in_the_dashboard_url() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    fs::create_dir(&pontia_home).expect("create pontia home");
    fs::write(
        pontia_home.join("config.toml"),
        "external_api_token = \"token with spaces&separator\"\n",
    )
    .expect("write config");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"\n\n".to_vec());
    let mut output = Vec::new();

    let outcome = run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("initialize Pontia")
    .expect("initialization completed");

    assert!(
        outcome
            .local_dashboard_url
            .ends_with("?token=token+with+spaces%26separator")
    );
}

#[test]
fn command_scoped_token_is_warned_about_and_never_used_as_daemon_credential() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform::default();
    let mut environment = vars(&user_home, &pontia_home);
    environment.insert(
        "PONTIA_EXTERNAL_API_TOKEN".to_string(),
        "command-only-secret".to_string(),
    );
    let mut input = Cursor::new(b"\n\n".to_vec());
    let mut output = Vec::new();

    let outcome = run(&mut input, &mut output, &environment, &platform)
        .expect("initialize Pontia")
        .expect("initialization completed");

    let config = fs::read_to_string(pontia_home.join("config.toml")).expect("read config");
    assert!(!config.contains("command-only-secret"));
    assert!(!outcome.local_dashboard_url.contains("command-only-secret"));
    let output = String::from_utf8(output).expect("UTF-8 output");
    assert!(output.contains("PONTIA_EXTERNAL_API_TOKEN"));
    assert!(!output.contains("command-only-secret"));
}

#[test]
fn invalid_existing_config_errors_do_not_echo_a_token() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    fs::create_dir(&pontia_home).expect("create pontia home");
    fs::write(
        pontia_home.join("config.toml"),
        "external_api_token = \"must-not-leak\"\ninvalid = [\n",
    )
    .expect("write invalid config");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();

    let error = run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect_err("invalid config must fail");

    assert!(error.contains("failed to load Pontia configuration"));
    assert!(!error.contains("must-not-leak"));
}

#[test]
fn ephemeral_bind_port_is_rejected_before_any_side_effect() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    fs::create_dir(&pontia_home).expect("create pontia home");
    fs::write(
        pontia_home.join("config.toml"),
        "bind_addr = \"127.0.0.1:0\"\n",
    )
    .expect("write config");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(Vec::<u8>::new());
    let mut output = Vec::new();

    let error = run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect_err("port zero must be rejected");

    assert!(error.contains("bind_addr"));
    assert!(error.contains("port 0"));
    assert!(platform.events.borrow().is_empty());
}

#[test]
fn cancellation_before_confirmation_has_no_side_effects() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"\nn\n".to_vec());
    let mut output = Vec::new();

    run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("cancel cleanly");

    assert!(platform.events.borrow().is_empty());
    assert!(!pontia_home.join("config.toml").exists());
}

#[test]
fn failed_pi_install_does_not_write_config_or_start_service() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform {
        install_error: Some("pi install failed"),
        ..FakePlatform::default()
    };
    let mut input = Cursor::new(b"\n\n".to_vec());
    let mut output = Vec::new();

    let error = run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect_err("installation must fail");

    assert_eq!(error, "pi install failed");
    assert_eq!(
        platform.events.borrow().as_slice(),
        ["preflight", "install-pi"]
    );
    assert!(!pontia_home.join("config.toml").exists());
}

#[test]
fn unavailable_dashboard_keeps_started_service_without_returning_a_token_url() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform {
        dashboard_ready: false,
        ..FakePlatform::default()
    };
    let mut input = Cursor::new(b"\n\n".to_vec());
    let mut output = Vec::new();

    let error = run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect_err("Dashboard must be available");

    assert!(error.contains("Dashboard is not available"));
    assert!(
        platform
            .events
            .borrow()
            .iter()
            .any(|event| event.starts_with("start:"))
    );
    assert!(
        !String::from_utf8(output)
            .expect("UTF-8 output")
            .contains("Dashboard: http")
    );
}

#[test]
fn initialization_only_consumes_agent_selection_and_confirmation_input() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"\n\nnot consumed\n".to_vec());
    let mut output = Vec::new();

    let outcome = run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("initialize Pontia")
    .expect("initialization completed");

    assert!(
        outcome
            .local_dashboard_url
            .starts_with("http://127.0.0.1:8080/dashboard?token=")
    );
    assert_eq!(input.position(), 2);
}

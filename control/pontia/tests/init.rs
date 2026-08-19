use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs,
    io::Cursor,
    net::SocketAddr,
    path::Path,
};

use pontia::init::{InitPlatform, run};
use pontia_config::AppConfig;

struct FakePlatform {
    events: RefCell<Vec<String>>,
    opened_urls: RefCell<Vec<String>>,
    install_error: Option<&'static str>,
    dashboard_ready: bool,
    browser_failures: Cell<usize>,
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self {
            events: RefCell::new(Vec::new()),
            opened_urls: RefCell::new(Vec::new()),
            install_error: None,
            dashboard_ready: true,
            browser_failures: Cell::new(0),
        }
    }
}

impl InitPlatform for FakePlatform {
    fn preflight(&self, install_pi: bool) -> Result<(), String> {
        self.events
            .borrow_mut()
            .push(format!("preflight:{install_pi}"));
        Ok(())
    }

    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), String> {
        bytes.fill(7);
        Ok(())
    }

    fn install_pi(&self) -> Result<(), String> {
        self.events.borrow_mut().push("install-pi".to_string());
        match self.install_error {
            Some(error) => Err(error.to_string()),
            None => Ok(()),
        }
    }

    fn start_service(&self, config: &AppConfig, config_changed: bool) -> Result<(), String> {
        self.events.borrow_mut().push(format!(
            "start:{}:{config_changed}",
            config.pontia_home.display()
        ));
        Ok(())
    }

    fn dashboard_available(&self, addr: SocketAddr) -> Result<bool, String> {
        self.events
            .borrow_mut()
            .push(format!("dashboard-ready:{addr}"));
        Ok(self.dashboard_ready)
    }

    fn open_browser(&self, url: &str) -> Result<(), String> {
        self.opened_urls.borrow_mut().push(url.to_string());
        let remaining = self.browser_failures.get();
        if remaining > 0 {
            self.browser_failures.set(remaining - 1);
            Err("browser unavailable".to_string())
        } else {
            Ok(())
        }
    }
}

fn vars(home: &Path, pontia_home: &Path) -> HashMap<String, String> {
    HashMap::from([
        ("HOME".to_string(), home.display().to_string()),
        ("PONTIA_HOME".to_string(), pontia_home.display().to_string()),
    ])
}

#[test]
fn default_initialization_installs_pi_writes_config_starts_service_and_opens_dashboard() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"\n\n\n".to_vec());
    let mut output = Vec::new();

    run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("initialize Pontia");

    let config_text = fs::read_to_string(pontia_home.join("config.toml")).expect("read config");
    let config: toml::Value = toml::from_str(&config_text).expect("valid TOML");
    let token = config["external_api_token"].as_str().expect("token");
    assert_eq!(token.len(), 43);
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
            "preflight:true",
            "install-pi",
            &format!("start:{}:true", pontia_home.display()),
            "dashboard-ready:127.0.0.1:8080",
        ]
    );
    let expected_url = format!("http://127.0.0.1:8080/dashboard?token={token}");
    assert_eq!(
        platform.opened_urls.borrow().as_slice(),
        [expected_url.as_str()]
    );

    let output = String::from_utf8(output).expect("UTF-8 output");
    assert!(output.contains("Pontia initialization"));
    assert!(output.contains("Installed pi integration"));
    assert!(output.contains(&format!("Dashboard: {expected_url}")));
    assert!(output.contains("Pontia will keep running"));
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
        let mut input = Cursor::new(b"\n\n\n".to_vec());
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
                .contains(&format!("start:{}:false", pontia_home.display()))
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
    let mut input = Cursor::new(b"\n\n\n".to_vec());
    let mut output = Vec::new();

    run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("initialize Pontia");

    assert!(platform.opened_urls.borrow()[0].ends_with("?token=token+with+spaces%26separator"));
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
    let mut input = Cursor::new(b"\n\n\n".to_vec());
    let mut output = Vec::new();

    run(&mut input, &mut output, &environment, &platform).expect("initialize Pontia");

    let config = fs::read_to_string(pontia_home.join("config.toml")).expect("read config");
    assert!(!config.contains("command-only-secret"));
    assert!(
        platform
            .opened_urls
            .borrow()
            .iter()
            .all(|url| !url.contains("command-only-secret"))
    );
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
    let mut input = Cursor::new(b"\n\nn\n".to_vec());
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
    assert!(
        String::from_utf8(output)
            .expect("UTF-8 output")
            .contains("Initialization cancelled.")
    );
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
    let mut input = Cursor::new(b"\n\n\n".to_vec());
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
        ["preflight:true", "install-pi"]
    );
    assert!(!pontia_home.join("config.toml").exists());
}

#[test]
fn unavailable_dashboard_keeps_started_service_but_does_not_print_or_open_token_url() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform {
        dashboard_ready: false,
        ..FakePlatform::default()
    };
    let mut input = Cursor::new(b"\n\n\n".to_vec());
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
    assert!(platform.opened_urls.borrow().is_empty());
    assert!(
        !String::from_utf8(output)
            .expect("UTF-8 output")
            .contains("Dashboard: http")
    );
}

#[test]
fn a_failed_initial_browser_open_can_be_retried_with_enter() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform {
        browser_failures: Cell::new(1),
        ..FakePlatform::default()
    };
    let mut input = Cursor::new(b"\n\n\n\n".to_vec());
    let mut output = Vec::new();

    run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("browser failure is non-fatal");

    assert_eq!(platform.opened_urls.borrow().len(), 2);
    let output = String::from_utf8(output).expect("UTF-8 output");
    assert!(output.contains("Could not open Dashboard: browser unavailable"));
    assert!(output.contains("✓ Opened Dashboard in your browser"));
}

#[test]
fn reopening_dashboard_uses_the_same_url_until_input_ends() {
    let dir = tempfile::tempdir().expect("temp dir");
    let user_home = dir.path().join("home");
    let pontia_home = dir.path().join("pontia");
    fs::create_dir(&user_home).expect("create home");
    let platform = FakePlatform::default();
    let mut input = Cursor::new(b"\n\n\n\n\n".to_vec());
    let mut output = Vec::new();

    run(
        &mut input,
        &mut output,
        &vars(&user_home, &pontia_home),
        &platform,
    )
    .expect("initialize Pontia");

    let opened = platform.opened_urls.borrow();
    assert_eq!(opened.len(), 3);
    assert!(opened.iter().all(|url| url == &opened[0]));
}

use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    sync::{Mutex, MutexGuard},
};

use pontia_config::{AppConfig, FilePickerConfig, RuntimeClientConfig, RuntimeConfig};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct EnvGuard {
    _lock: MutexGuard<'static, ()>,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn lock(keys: &[&'static str]) -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        Self {
            _lock: lock,
            saved: keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.saved.iter().rev() {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

fn vars_for_home(home: &std::path::Path) -> HashMap<String, String> {
    HashMap::from([("PONTIA_HOME".to_string(), home.display().to_string())])
}

#[test]
fn loads_config_from_key_value_source() {
    let home = tempfile::tempdir().expect("Pontia home");
    let vars = HashMap::from([
        ("PONTIA_HOME".to_string(), home.path().display().to_string()),
        (
            "PONTIA_DASHBOARD_SOURCE".to_string(),
            "https://example.test/dashboard.tar.gz".to_string(),
        ),
        (
            "PONTIA_EXTERNAL_API_TOKEN".to_string(),
            "dev-token".to_string(),
        ),
        ("PONTIA_RUN_MIGRATIONS".to_string(), "false".to_string()),
        ("PONTIA_DEFAULT_CLIENT_TYPE".to_string(), "pi".to_string()),
        (
            "PONTIA_WORKSPACE_ROOTS".to_string(),
            "projects|Projects|/home/me/projects;tmp|Temporary|/tmp".to_string(),
        ),
    ]);

    let config = AppConfig::from_vars(&vars).expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:8080");
    assert_eq!(
        config.database_url,
        format!(
            "sqlite://{}",
            home.path().join("data/pontia-e1.db").display()
        )
    );
    assert_eq!(config.external_api_token.as_deref(), Some("dev-token"));
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some("https://example.test/dashboard.tar.gz")
    );
    assert!(!config.run_migrations);
    assert_eq!(config.default_client_type, "pi");
    assert_eq!(config.workspace_browser.roots.len(), 2);
    assert_eq!(config.workspace_browser.roots[0].root_id, "projects");
    assert_eq!(config.workspace_browser.roots[0].label, "Projects");
    assert_eq!(config.workspace_browser.roots[0].path, "/home/me/projects");
    assert_eq!(config.file_picker, FilePickerConfig::default());
}

#[test]
fn file_picker_uses_built_in_defaults_when_not_configured() {
    let home = tempfile::tempdir().expect("Pontia home");
    let config = AppConfig::from_vars(&vars_for_home(home.path())).expect("config should load");

    assert!(config.file_picker.enabled);
    assert_eq!(config.file_picker.min_query_chars, 0);
    assert_eq!(config.file_picker.max_results, 100);
    assert_eq!(config.file_picker.max_candidates, 100_000);
    assert_eq!(config.file_picker.timeout_ms, 1_500);
    assert!(config.file_picker.respect_gitignore);
    assert!(config.file_picker.respect_ignore_files);
    assert!(config.file_picker.respect_git_exclude);
    assert!(!config.file_picker.include_hidden);
    assert!(!config.file_picker.follow_symlinks);
    assert!(
        config
            .file_picker
            .ignore_globs
            .contains(&"node_modules/**".to_string())
    );
}

#[test]
fn file_picker_config_file_overrides_only_specified_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    fs::write(
        &config_path,
        r#"
[file_picker]
include_hidden = true
respect_gitignore = false
ignore_globs = []
"#,
    )
    .expect("write config");

    let config = AppConfig::from_vars(&vars_for_home(dir.path())).expect("config should load");

    assert!(config.file_picker.include_hidden);
    assert!(!config.file_picker.respect_gitignore);
    assert!(config.file_picker.respect_ignore_files);
    assert_eq!(config.file_picker.max_results, 100);
    assert!(config.file_picker.ignore_globs.is_empty());
}

#[test]
fn loads_config_from_toml_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    fs::write(
        &config_path,
        r#"
bind_addr = "127.0.0.1:4040"
external_api_token = "file-token"
run_migrations = false
default_client_type = "pi"

[dashboard]
source = "/opt/pontia/dashboard"

[runtime.pi]
tui_command = "custom-pi from file"

[runtime.custom]
tui_command = "custom-agent from file"

[workspace_browser]
roots = [
  { root_id = "projects", label = "Projects", path = "/home/me/projects" }
]
"#,
    )
    .expect("write config");

    let config = AppConfig::from_vars(&vars_for_home(dir.path())).expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4040");
    assert_eq!(
        config.database_url,
        format!(
            "sqlite://{}",
            dir.path().join("data/pontia-e1.db").display()
        )
    );
    assert_eq!(config.external_api_token.as_deref(), Some("file-token"));
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some("/opt/pontia/dashboard")
    );
    assert!(!config.run_migrations);
    assert_eq!(config.default_client_type, "pi");
    assert_eq!(
        config
            .runtime
            .tui_command_for_client_config_key("pi")
            .as_deref(),
        Some("custom-pi from file")
    );
    assert_eq!(
        config
            .runtime
            .tui_command_for_client_config_key("custom")
            .as_deref(),
        Some("custom-agent from file")
    );
    assert_eq!(config.workspace_browser.roots.len(), 1);
    assert_eq!(config.workspace_browser.roots[0].root_id, "projects");
}

#[test]
fn env_vars_override_config_file_values() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    fs::write(
        &config_path,
        r#"
bind_addr = "127.0.0.1:4040"
external_api_token = "file-token"
default_client_type = "pi"

[dashboard]
source = "/from/file/dashboard"

[runtime.pi]
tui_command = "pi from file"
"#,
    )
    .expect("write config");
    let vars = HashMap::from([
        ("PONTIA_HOME".to_string(), dir.path().display().to_string()),
        (
            "PONTIA_EXTERNAL_API_TOKEN".to_string(),
            "env-token".to_string(),
        ),
        (
            "PONTIA_PI_TUI_COMMAND".to_string(),
            "pi from env".to_string(),
        ),
        (
            "PONTIA_DASHBOARD_SOURCE".to_string(),
            "/from/env/dashboard".to_string(),
        ),
        ("PONTIA_DEFAULT_CLIENT_TYPE".to_string(), "pi".to_string()),
    ]);

    let config = AppConfig::from_vars(&vars).expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4040");
    assert_eq!(config.external_api_token.as_deref(), Some("env-token"));
    assert_eq!(
        config
            .runtime
            .tui_command_for_client_config_key("pi")
            .as_deref(),
        Some("pi from env")
    );
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some("/from/env/dashboard")
    );
    assert_eq!(config.default_client_type, "pi");
}

#[test]
fn runtime_config_resolves_tui_commands_by_agent_client_config_key() {
    let config = RuntimeConfig {
        clients: HashMap::from([(
            "pi".to_string(),
            RuntimeClientConfig {
                tui_command: Some("pi from config".to_string()),
            },
        )]),
    };

    assert_eq!(
        config.tui_command_for_client_config_key("pi").as_deref(),
        Some("pi from config")
    );
    assert_eq!(config.tui_command_for_client_config_key("unknown"), None);
}

#[test]
fn rejects_generic_as_default_client_type() {
    let home = tempfile::tempdir().expect("Pontia home");
    let vars = HashMap::from([
        ("PONTIA_HOME".to_string(), home.path().display().to_string()),
        (
            "PONTIA_DEFAULT_CLIENT_TYPE".to_string(),
            "generic".to_string(),
        ),
    ]);

    let error = AppConfig::from_vars(&vars).expect_err("generic default should fail");

    assert!(error.to_string().contains("default client type must be pi"));
}

#[test]
fn rejects_missing_empty_relative_and_tilde_prefixed_pontia_home() {
    for (name, vars) in [
        ("missing", HashMap::new()),
        (
            "empty",
            HashMap::from([("PONTIA_HOME".to_string(), "   ".to_string())]),
        ),
        (
            "relative",
            HashMap::from([("PONTIA_HOME".to_string(), "var/pontia".to_string())]),
        ),
        (
            "tilde",
            HashMap::from([("PONTIA_HOME".to_string(), "~/.pontia".to_string())]),
        ),
        (
            "filesystem-root",
            HashMap::from([("PONTIA_HOME".to_string(), "/".to_string())]),
        ),
    ] {
        let error = AppConfig::from_vars(&vars).expect_err(name);
        assert!(error.to_string().contains("PONTIA_HOME"), "{name}: {error}");
    }
}

#[test]
fn pontia_home_overrides_development_default_data_paths() {
    let home = tempfile::tempdir().expect("Pontia home");
    let config = AppConfig::from_vars(&vars_for_home(home.path())).expect("defaults load");

    assert_eq!(
        config.database_url,
        format!(
            "sqlite://{}",
            home.path().join("data/pontia-e1.db").display()
        )
    );
}

#[test]
fn pontia_home_overrides_default_config_file_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    fs::write(
        &config_path,
        r#"
bind_addr = "127.0.0.1:4545"
external_api_token = "home-config-token"
"#,
    )
    .expect("write config");

    let _env = EnvGuard::lock(&["PONTIA_HOME", "PONTIA_EXTERNAL_API_TOKEN"]);
    unsafe {
        std::env::set_var("PONTIA_HOME", dir.path());
        std::env::remove_var("PONTIA_EXTERNAL_API_TOKEN");
    }

    let config = AppConfig::from_env().expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4545");
    assert_eq!(
        config.external_api_token.as_deref(),
        Some("home-config-token")
    );
}

#[test]
fn provides_development_defaults_for_optional_values() {
    let home = tempfile::tempdir().expect("Pontia home");
    let config = AppConfig::from_vars(&vars_for_home(home.path())).expect("defaults load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:8080");
    assert_eq!(
        config.database_url,
        format!(
            "sqlite://{}",
            home.path().join("data/pontia-e1.db").display()
        )
    );
    assert_eq!(config.external_api_token, None);
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some(concat!(
            "https://github.com/anthod0/pontia/releases/download/v",
            env!("CARGO_PKG_VERSION"),
            "/pontia-dashboard-v",
            env!("CARGO_PKG_VERSION"),
            ".zip"
        ))
    );
    assert!(config.run_migrations);
    assert_eq!(config.default_client_type, "pi");
    assert!(config.workspace_browser.roots.is_empty());
}

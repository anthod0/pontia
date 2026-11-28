use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    sync::{Mutex, MutexGuard},
};

use pontia_config::{AppConfig, FilePickerConfig};

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
    assert_eq!(config.auth_origin, "https://pontia.dev");
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some("https://example.test/dashboard.tar.gz")
    );
    assert!(!config.run_migrations);
    assert_eq!(config.workspace_browser.roots.len(), 2);
    assert_eq!(config.workspace_browser.roots[0].root_id, "projects");
    assert_eq!(config.workspace_browser.roots[0].label, "Projects");
    assert_eq!(config.workspace_browser.roots[0].path, "/home/me/projects");
    assert_eq!(config.file_picker, FilePickerConfig::default());
}

#[test]
fn auth_origin_uses_the_cli_override_and_requires_a_clean_https_origin() {
    let home = tempfile::tempdir().expect("Pontia home");
    let mut vars = vars_for_home(home.path());
    vars.insert(
        "PONTIA_AUTH_ORIGIN".to_string(),
        "https://cloud.example/development".to_string(),
    );
    assert_eq!(
        AppConfig::from_vars(&vars).unwrap().auth_origin,
        "https://cloud.example"
    );

    for invalid in [
        "http://cloud.example",
        "https://user@cloud.example",
        "https://cloud.example?query=true",
    ] {
        vars.insert("PONTIA_AUTH_ORIGIN".to_string(), invalid.to_string());
        assert!(AppConfig::from_vars(&vars).is_err(), "{invalid}");
    }
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

[dashboard]
source = "/opt/pontia/dashboard"

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

[dashboard]
source = "/from/file/dashboard"
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
            "PONTIA_DASHBOARD_SOURCE".to_string(),
            "/from/env/dashboard".to_string(),
        ),
    ]);

    let config = AppConfig::from_vars(&vars).expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4040");
    assert_eq!(config.external_api_token.as_deref(), Some("env-token"));
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some("/from/env/dashboard")
    );
}

#[test]
fn rejects_invalid_explicit_pontia_home() {
    for (name, vars) in [
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
fn omitted_roots_use_current_home_without_persisting_defaults() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    for contents in [
        None,
        Some("bind_addr = '127.0.0.1:9090'\n"),
        Some("[workspace_browser]\n"),
    ] {
        if let Some(contents) = contents {
            fs::write(&path, contents).unwrap();
        }
        let mut vars = vars_for_home(root.path());
        for name in ["first-home", "second-home"] {
            let home = root.path().join(name);
            vars.insert("HOME".into(), home.display().to_string());
            let config = AppConfig::from_vars(&vars).unwrap();
            assert_eq!(config.workspace_browser.roots.len(), 1);
            let loaded = &config.workspace_browser.roots[0];
            assert_eq!(loaded.root_id, "home");
            assert_eq!(loaded.label, "Home");
            assert_eq!(loaded.path, home.to_str().unwrap());
        }
        assert_eq!(fs::read_to_string(&path).ok().as_deref(), contents);
    }
}

#[test]
fn explicit_file_and_environment_roots_override_home_default() {
    let root = tempfile::tempdir().unwrap();
    let mut vars = vars_for_home(root.path());
    vars.insert("HOME".into(), root.path().display().to_string());
    for roots in [
        "[]",
        "[{ root_id = 'work', label = 'Work', path = '/work' }]",
    ] {
        fs::write(
            root.path().join("config.toml"),
            format!("[workspace_browser]\nroots = {roots}\n"),
        )
        .unwrap();
        let config = AppConfig::from_vars(&vars).unwrap();
        if roots == "[]" {
            assert!(config.workspace_browser.roots.is_empty());
        } else {
            assert_eq!(config.workspace_browser.roots.len(), 1);
            assert_eq!(config.workspace_browser.roots[0].root_id, "work");
            assert_eq!(config.workspace_browser.roots[0].path, "/work");
        }
    }
    vars.insert("PONTIA_WORKSPACE_ROOTS".into(), String::new());
    assert!(
        AppConfig::from_vars(&vars)
            .unwrap()
            .workspace_browser
            .roots
            .is_empty()
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
    let dashboard_source = format!(
        "https://get.pontia.dev/releases/v{}/pontia-dashboard.tar.gz",
        pontia_version::version()
    );
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some(dashboard_source.as_str())
    );
    assert!(config.run_migrations);
    assert!(config.workspace_browser.roots.is_empty());
    assert!(config.remote.is_none());
}

#[test]
fn remote_registration_configures_only_the_device_id() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("config.toml"),
        "[remote]\ndevice_id = '0195e7c1-1b22-7c33-9d44-123456789abc'\n",
    )
    .unwrap();
    let remote = AppConfig::from_vars(&vars_for_home(root.path()))
        .unwrap()
        .remote
        .unwrap();
    assert_eq!(
        remote.device_id.as_deref(),
        Some("0195e7c1-1b22-7c33-9d44-123456789abc")
    );
}

#[test]
fn legacy_remote_connection_fields_are_ignored() {
    for field in [
        "edge_url = 'wss://edge.example/tunnel'",
        "ca_certificate = '/opt/pontia/ca.pem'",
    ] {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("config.toml"),
            format!("[remote]\ndevice_id = '0195e7c1-1b22-7c33-9d44-123456789abc'\n{field}\n"),
        )
        .unwrap();
        let remote = AppConfig::from_vars(&vars_for_home(root.path()))
            .unwrap()
            .remote
            .unwrap();
        assert_eq!(
            remote.device_id.as_deref(),
            Some("0195e7c1-1b22-7c33-9d44-123456789abc")
        );
    }
}

#[test]
fn loads_preview_defaults_and_device_overrides() {
    let root = tempfile::tempdir().unwrap();
    let vars = vars_for_home(root.path());
    let file = root.path().join("config.toml");
    let defaults = AppConfig::from_vars(&vars).unwrap().preview;
    assert!(defaults.enabled);
    assert!(defaults.allowed_ports.is_empty());

    fs::write(&file, "[preview]\nallowed_ports = [9222, 2376]").unwrap();
    let preview = AppConfig::from_vars(&vars).unwrap().preview;
    assert!(preview.enabled);
    assert_eq!(preview.allowed_ports, [9222, 2376]);

    fs::write(file, "[preview]\nenabled = false").unwrap();
    assert!(!AppConfig::from_vars(&vars).unwrap().preview.enabled);
}

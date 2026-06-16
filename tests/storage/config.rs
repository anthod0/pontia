use std::{collections::HashMap, fs};

use pontia::config::{AppConfig, config_path_from_args};

#[test]
fn loads_config_from_key_value_source() {
    let vars = HashMap::from([
        ("PONTIA_BIND_ADDR".to_string(), "127.0.0.1:4000".to_string()),
        (
            "PONTIA_DATABASE_URL".to_string(),
            "sqlite://./data/control-plane.db".to_string(),
        ),
        (
            "PONTIA_DASHBOARD_SOURCE".to_string(),
            "https://example.test/dashboard.tar.gz".to_string(),
        ),
        (
            "PONTIA_DASHBOARD_CACHE_DIR".to_string(),
            "/tmp/pontia-dashboard-cache".to_string(),
        ),
        (
            "PONTIA_EXTERNAL_API_TOKEN".to_string(),
            "dev-token".to_string(),
        ),
        ("PONTIA_RUN_MIGRATIONS".to_string(), "false".to_string()),
        (
            "PONTIA_DEFAULT_CLIENT_TYPE".to_string(),
            "claude_code".to_string(),
        ),
        ("PONTIA_GRAPH_ENABLED".to_string(), "true".to_string()),
        (
            "PONTIA_GRAPH_DB_DIR".to_string(),
            "/tmp/pontia-graph".to_string(),
        ),
        (
            "PONTIA_WORKSPACE_ROOTS".to_string(),
            "projects|Projects|/home/me/projects;tmp|Temporary|/tmp".to_string(),
        ),
    ]);

    let config = AppConfig::from_vars(&vars).expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4000");
    assert_eq!(config.database_url, "sqlite://./data/control-plane.db");
    assert_eq!(config.external_api_token.as_deref(), Some("dev-token"));
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some("https://example.test/dashboard.tar.gz")
    );
    assert_eq!(
        config.dashboard.cache_dir.as_deref(),
        Some("/tmp/pontia-dashboard-cache")
    );
    assert!(!config.run_migrations);
    assert_eq!(config.default_client_type, "claude_code");
    assert!(config.graph.enabled);
    assert_eq!(config.graph.db_dir.as_deref(), Some("/tmp/pontia-graph"));
    assert_eq!(config.workspace_browser.roots.len(), 2);
    assert_eq!(config.workspace_browser.roots[0].root_id, "projects");
    assert_eq!(config.workspace_browser.roots[0].label, "Projects");
    assert_eq!(config.workspace_browser.roots[0].path, "/home/me/projects");
}

#[test]
fn graph_enabled_defaults_db_dir_next_to_sqlite_data_file() {
    let vars = HashMap::from([
        (
            "PONTIA_DATABASE_URL".to_string(),
            "sqlite:///tmp/pontia/control.db".to_string(),
        ),
        ("PONTIA_GRAPH_ENABLED".to_string(), "true".to_string()),
    ]);

    let config = AppConfig::from_vars(&vars).expect("config should load");

    assert!(config.graph.enabled);
    assert_eq!(
        config.graph.db_dir.as_deref(),
        Some("/tmp/pontia/graph/lbug")
    );
}

#[test]
fn loads_config_from_toml_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let config_path = dir.path().join("config.toml");
    fs::write(
        &config_path,
        r#"
bind_addr = "127.0.0.1:4040"
database_url = "sqlite:///tmp/from-file.db"
external_api_token = "file-token"
run_migrations = false
default_client_type = "claude_code"

[dashboard]
source = "/opt/pontia/dashboard"
cache_dir = "/var/cache/pontia/dashboard"

[runtime.pi]
tui_command = "pi --approve -e /tmp/pontia/clients/pi"

[runtime.claude_code]
tui_command = "claude --dangerously-skip-permissions"

[workspace_browser]
roots = [
  { root_id = "projects", label = "Projects", path = "/home/me/projects" }
]
"#,
    )
    .expect("write config");

    let config = AppConfig::from_vars_and_file(&HashMap::new(), Some(&config_path))
        .expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4040");
    assert_eq!(config.database_url, "sqlite:///tmp/from-file.db");
    assert_eq!(config.external_api_token.as_deref(), Some("file-token"));
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some("/opt/pontia/dashboard")
    );
    assert_eq!(
        config.dashboard.cache_dir.as_deref(),
        Some("/var/cache/pontia/dashboard")
    );
    assert!(!config.run_migrations);
    assert_eq!(config.default_client_type, "claude_code");
    assert_eq!(
        config.runtime.pi.tui_command.as_deref(),
        Some("pi --approve -e /tmp/pontia/clients/pi")
    );
    assert_eq!(
        config.runtime.claude_code.tui_command.as_deref(),
        Some("claude --dangerously-skip-permissions")
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
default_client_type = "claude_code"

[dashboard]
source = "/from/file/dashboard"
cache_dir = "/from/file/cache"

[runtime.pi]
tui_command = "pi from file"
"#,
    )
    .expect("write config");
    let vars = HashMap::from([
        ("PONTIA_BIND_ADDR".to_string(), "127.0.0.1:5050".to_string()),
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
        (
            "PONTIA_DASHBOARD_CACHE_DIR".to_string(),
            "/from/env/cache".to_string(),
        ),
        ("PONTIA_DEFAULT_CLIENT_TYPE".to_string(), "pi".to_string()),
    ]);

    let config =
        AppConfig::from_vars_and_file(&vars, Some(&config_path)).expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:5050");
    assert_eq!(config.external_api_token.as_deref(), Some("env-token"));
    assert_eq!(
        config.runtime.pi.tui_command.as_deref(),
        Some("pi from env")
    );
    assert_eq!(
        config.dashboard.source.as_deref(),
        Some("/from/env/dashboard")
    );
    assert_eq!(
        config.dashboard.cache_dir.as_deref(),
        Some("/from/env/cache")
    );
    assert_eq!(config.default_client_type, "pi");
}

#[test]
fn rejects_generic_as_default_client_type() {
    let vars = HashMap::from([(
        "PONTIA_DEFAULT_CLIENT_TYPE".to_string(),
        "generic".to_string(),
    )]);

    let error = AppConfig::from_vars(&vars).expect_err("generic default should fail");

    assert!(
        error
            .to_string()
            .contains("default client type must be pi or claude_code")
    );
}

#[test]
fn parses_config_path_from_cli_args() {
    let path = config_path_from_args([
        "pontia".to_string(),
        "--config".to_string(),
        "/tmp/pontia.toml".to_string(),
    ])
    .expect("parse args");

    assert_eq!(
        path.as_deref(),
        Some(std::path::Path::new("/tmp/pontia.toml"))
    );
}

#[test]
fn rejects_config_arg_without_path() {
    let error = config_path_from_args(["pontia".to_string(), "--config".to_string()])
        .expect_err("missing path should fail");

    assert!(error.to_string().contains("--config requires a path"));
}

#[test]
fn provides_development_defaults_for_optional_values() {
    let config = AppConfig::from_vars(&HashMap::<String, String>::new()).expect("defaults load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:8080");
    assert_eq!(
        config.database_url,
        "sqlite://~/.local/share/pontia/pontia.db"
    );
    assert_eq!(config.external_api_token, None);
    assert_eq!(config.dashboard.source, None);
    assert_eq!(config.dashboard.cache_dir, None);
    assert!(config.run_migrations);
    assert_eq!(config.default_client_type, "pi");
    assert!(config.graph.enabled);
    assert_eq!(
        config.graph.db_dir.as_deref(),
        Some("~/.local/share/pontia/graph/lbug")
    );
    assert!(config.workspace_browser.roots.is_empty());
}

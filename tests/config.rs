use std::collections::HashMap;

use llmparty::config::AppConfig;

#[test]
fn loads_config_from_key_value_source() {
    let vars = HashMap::from([
        (
            "LLMPARTY_BIND_ADDR".to_string(),
            "127.0.0.1:4000".to_string(),
        ),
        (
            "LLMPARTY_DATABASE_URL".to_string(),
            "sqlite://./data/control-plane.db".to_string(),
        ),
        (
            "LLMPARTY_EXTERNAL_API_TOKEN".to_string(),
            "dev-token".to_string(),
        ),
        ("LLMPARTY_RUN_MIGRATIONS".to_string(), "false".to_string()),
        ("LLMPARTY_PLANNER_ENABLED".to_string(), "true".to_string()),
        (
            "LLMPARTY_PLANNER_CLIENT_TYPE".to_string(),
            "generic".to_string(),
        ),
        (
            "LLMPARTY_PLANNER_TIMEOUT_MS".to_string(),
            "12000".to_string(),
        ),
        (
            "LLMPARTY_PLANNER_COMPAT_DIRECT_DISPATCH".to_string(),
            "true".to_string(),
        ),
    ]);

    let config = AppConfig::from_vars(&vars).expect("config should load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4000");
    assert_eq!(config.database_url, "sqlite://./data/control-plane.db");
    assert_eq!(config.external_api_token.as_deref(), Some("dev-token"));
    assert!(!config.run_migrations);
    assert!(config.planner.enabled);
    assert_eq!(config.planner.client_type, "generic");
    assert_eq!(config.planner.timeout_ms, 12_000);
    assert!(config.planner.compatibility_direct_dispatch);
}

#[test]
fn provides_development_defaults_for_optional_values() {
    let config = AppConfig::from_vars(&HashMap::<String, String>::new()).expect("defaults load");

    assert_eq!(config.bind_addr.to_string(), "127.0.0.1:8080");
    assert_eq!(
        config.database_url,
        "sqlite://~/.local/share/llmparty/llmparty.db"
    );
    assert_eq!(config.external_api_token, None);
    assert!(config.run_migrations);
    assert!(!config.planner.enabled);
    assert_eq!(config.planner.client_type, "pi");
    assert_eq!(config.planner.timeout_ms, 30_000);
    assert!(!config.planner.compatibility_direct_dispatch);
}

use pontia_config::AppConfig;
use std::collections::HashMap;

#[test]
fn bot_context_limits_load_from_the_config_file() {
    let root = tempfile::tempdir().unwrap();
    let vars = HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]);
    let defaults = AppConfig::from_vars(&vars, &[]).unwrap().bot;
    assert_eq!(defaults.context_token_limit.get(), 258_000);
    assert_eq!(defaults.context_window_percent, 80);
    std::fs::write(
        root.path().join("config.toml"),
        "[bot]\ncontext_token_limit = 180000\ncontext_window_percent = 70\n",
    )
    .unwrap();
    let configured = AppConfig::from_vars(&vars, &[]).unwrap().bot;
    assert_eq!(configured.context_token_limit.get(), 180_000);
    assert_eq!(configured.context_window_percent, 70);
    std::fs::write(
        root.path().join("config.toml"),
        "[bot]\ncontext_token_limit = 0\n",
    )
    .unwrap();
    assert!(AppConfig::from_vars(&vars, &[]).is_err());
}

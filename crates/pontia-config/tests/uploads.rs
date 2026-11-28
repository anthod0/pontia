use std::collections::HashMap;

use pontia_config::AppConfig;

fn load(contents: &str) -> pontia_core::Result<AppConfig> {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("config.toml"), contents).unwrap();
    AppConfig::from_vars(&HashMap::from([(
        "PONTIA_HOME".into(),
        root.path().display().to_string(),
    )]))
}

#[test]
fn upload_retention_defaults_and_partial_overrides() {
    let defaults = load("").unwrap().uploads;
    assert_eq!(defaults.incomplete_retention_days.get(), 7);
    assert_eq!(defaults.completed_retention_days.get(), 30);
    assert_eq!(defaults.cleanup_interval_hours.get(), 24);

    let config = load("[uploads]\ncompleted_retention_days = 90\n")
        .unwrap()
        .uploads;
    assert_eq!(config.completed_retention_days.get(), 90);
    assert_eq!(config.incomplete_retention_days.get(), 7);
    assert_eq!(config.cleanup_interval_hours.get(), 24);

    let config = load("[uploads]\nincomplete_retention_days = 2\ncleanup_interval_hours = 6\n")
        .unwrap()
        .uploads;
    assert_eq!(config.incomplete_retention_days.get(), 2);
    assert_eq!(config.cleanup_interval_hours.get(), 6);
    assert_eq!(config.completed_retention_days.get(), 30);
}

#[test]
fn rejects_invalid_upload_retention_and_intervals() {
    for key in [
        "incomplete_retention_days",
        "completed_retention_days",
        "cleanup_interval_hours",
    ] {
        for value in ["0", "-1", "1.5", "4294967296", "'7'"] {
            assert!(
                load(&format!("[uploads]\n{key} = {value}\n")).is_err(),
                "{key} = {value}"
            );
        }
    }
}

#[test]
fn unknown_top_level_and_nested_keys_are_ignored() {
    let config = load(
        "future_setting = true\n\
         [future_table]\nkey = 1\n\
         [uploads]\nretention_days = 7\ncompleted_retention_days = 90\n\
         [bot]\nfuture_limit = 1\ncontext_window_percent = 70\n\
         [remote]\nfuture_field = \"x\"\n\
         [dashboard]\nfuture_field = 1\n\
         [file_picker]\nfuture_field = 1\n\
         [workspace_browser]\nfuture_field = 1\n",
    )
    .unwrap();
    assert_eq!(config.uploads.completed_retention_days.get(), 90);
    assert_eq!(config.uploads.incomplete_retention_days.get(), 7);
    assert_eq!(config.bot.context_window_percent, 70);
    assert_eq!(config.remote.unwrap().device_id, None);
}

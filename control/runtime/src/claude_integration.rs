use std::{
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

use pontia_core::error::{Error, Result};
use serde_json::{Map, Value};

const CLAUDE_SETTINGS_FILE: &str = ".claude/settings.json";

const PONTIA_OTEL_ENV: &[(&str, &str)] = &[
    ("CLAUDE_CODE_ENABLE_TELEMETRY", "1"),
    ("OTEL_LOGS_EXPORTER", "otlp"),
    ("OTEL_METRICS_EXPORTER", "none"),
    ("OTEL_TRACES_EXPORTER", "none"),
    ("OTEL_EXPORTER_OTLP_LOGS_PROTOCOL", "http/json"),
    ("OTEL_LOGS_EXPORT_INTERVAL", "1000"),
    ("OTEL_LOG_USER_PROMPTS", "0"),
    ("OTEL_LOG_ASSISTANT_RESPONSES", "0"),
    ("OTEL_LOG_TOOL_DETAILS", "0"),
    ("OTEL_LOG_TOOL_CONTENT", "0"),
    ("OTEL_LOG_RAW_API_BODIES", "0"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeApprovalIntegration {
    Configured { settings_path: PathBuf },
    SkippedMissingApiToken,
}

pub fn configure_claude_user_approval_integration(
    bind_addr: SocketAddr,
    api_token: Option<&str>,
) -> Result<ClaudeApprovalIntegration> {
    let Some(api_token) = api_token.filter(|token| !token.trim().is_empty()) else {
        return Ok(ClaudeApprovalIntegration::SkippedMissingApiToken);
    };
    let home = std::env::var_os("HOME").ok_or_else(|| Error::InvalidConfig {
        key: "HOME",
        message: "is required to configure Claude user settings".to_string(),
    })?;
    let settings_path = PathBuf::from(home).join(CLAUDE_SETTINGS_FILE);
    configure_claude_settings_file(&settings_path, bind_addr, api_token)?;
    Ok(ClaudeApprovalIntegration::Configured { settings_path })
}

fn configure_claude_settings_file(
    settings_path: &Path,
    bind_addr: SocketAddr,
    api_token: &str,
) -> Result<()> {
    let write_path = settings_write_path(settings_path)?;
    let mut settings = read_settings(&write_path)?;
    let root = settings
        .as_object_mut()
        .ok_or_else(|| invalid_settings("settings root must be a JSON object"))?;
    let env = env_object(root)?;

    for (key, value) in PONTIA_OTEL_ENV {
        env.insert((*key).to_string(), Value::String((*value).to_string()));
    }
    env.insert(
        "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT".to_string(),
        Value::String(format!(
            "http://127.0.0.1:{}/internal/v1/otel/v1/logs",
            bind_addr.port()
        )),
    );
    env.insert(
        "OTEL_EXPORTER_OTLP_HEADERS".to_string(),
        Value::String(format!("Authorization=Bearer {api_token}")),
    );

    write_settings_atomically(&write_path, &settings)
}

fn settings_write_path(settings_path: &Path) -> Result<PathBuf> {
    match fs::symlink_metadata(settings_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Ok(fs::canonicalize(settings_path)?),
        Ok(_) => Ok(settings_path.to_path_buf()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(settings_path.to_path_buf())
        }
        Err(error) => Err(error.into()),
    }
}

fn read_settings(settings_path: &Path) -> Result<Value> {
    match fs::read_to_string(settings_path) {
        Ok(contents) => serde_json::from_str(&contents).map_err(|error| {
            invalid_settings(format!(
                "{} contains invalid JSON: {error}",
                settings_path.display()
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(error) => Err(error.into()),
    }
}

fn env_object(root: &mut Map<String, Value>) -> Result<&mut Map<String, Value>> {
    let env = root
        .entry("env".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    env.as_object_mut()
        .ok_or_else(|| invalid_settings("Claude settings env must be a JSON object"))
}

fn write_settings_atomically(settings_path: &Path, settings: &Value) -> Result<()> {
    let parent = settings_path.parent().ok_or_else(|| {
        invalid_settings(format!(
            "{} has no parent directory",
            settings_path.display()
        ))
    })?;
    fs::create_dir_all(parent)?;

    let (temp_path, mut file) = create_temp_settings_file(parent)?;
    let result = (|| -> Result<()> {
        serde_json::to_writer_pretty(&mut file, settings)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        fs::rename(&temp_path, settings_path)?;
        fs::set_permissions(settings_path, fs::Permissions::from_mode(0o600))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn create_temp_settings_file(parent: &Path) -> Result<(PathBuf, fs::File)> {
    for attempt in 0..100 {
        let path = parent.join(format!(
            ".settings.json.pontia-{}-{attempt}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(Error::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a temporary Claude settings file",
    )))
}

fn invalid_settings(message: impl Into<String>) -> Error {
    Error::InvalidConfig {
        key: "CLAUDE_USER_SETTINGS",
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn merges_pontia_otel_logs_configuration_without_touching_other_settings() {
        let dir = tempdir().expect("tempdir");
        let settings_path = dir.path().join(".claude/settings.json");
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(
            &settings_path,
            serde_json::to_vec_pretty(&json!({
                "permissions": {"allow": ["Bash(pnpm test)"]},
                "hooks": {"Notification": [{"hooks": [{"type": "command", "command": "notify"}]}]},
                "env": {
                    "EXISTING": "preserved",
                    "OTEL_LOGS_EXPORTER": "console",
                    "OTEL_METRICS_EXPORTER": "otlp"
                }
            }))
            .unwrap(),
        )
        .unwrap();

        configure_claude_settings_file(
            &settings_path,
            "0.0.0.0:18080".parse().unwrap(),
            "pontia-token",
        )
        .unwrap();

        let settings: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        assert_eq!(
            settings["permissions"],
            json!({"allow": ["Bash(pnpm test)"]})
        );
        assert_eq!(
            settings["hooks"],
            json!({"Notification": [{"hooks": [{"type": "command", "command": "notify"}]}]})
        );
        assert_eq!(settings["env"]["EXISTING"], "preserved");
        assert_eq!(settings["env"]["CLAUDE_CODE_ENABLE_TELEMETRY"], "1");
        assert_eq!(settings["env"]["OTEL_LOGS_EXPORTER"], "otlp");
        assert_eq!(settings["env"]["OTEL_METRICS_EXPORTER"], "none");
        assert_eq!(settings["env"]["OTEL_TRACES_EXPORTER"], "none");
        assert_eq!(
            settings["env"]["OTEL_EXPORTER_OTLP_LOGS_PROTOCOL"],
            "http/json"
        );
        assert_eq!(
            settings["env"]["OTEL_EXPORTER_OTLP_LOGS_ENDPOINT"],
            "http://127.0.0.1:18080/internal/v1/otel/v1/logs"
        );
        assert_eq!(
            settings["env"]["OTEL_EXPORTER_OTLP_HEADERS"],
            "Authorization=Bearer pontia-token"
        );
        for key in [
            "OTEL_LOG_USER_PROMPTS",
            "OTEL_LOG_ASSISTANT_RESPONSES",
            "OTEL_LOG_TOOL_DETAILS",
            "OTEL_LOG_TOOL_CONTENT",
            "OTEL_LOG_RAW_API_BODIES",
        ] {
            assert_eq!(settings["env"][key], "0", "{key} must stay disabled");
        }
    }

    #[test]
    fn creates_user_settings_and_is_idempotent() {
        let dir = tempdir().expect("tempdir");
        let settings_path = dir.path().join(".claude/settings.json");
        let bind_addr = "127.0.0.1:8080".parse().unwrap();

        configure_claude_settings_file(&settings_path, bind_addr, "token").unwrap();
        let first = fs::read(&settings_path).unwrap();
        configure_claude_settings_file(&settings_path, bind_addr, "token").unwrap();

        assert_eq!(fs::read(&settings_path).unwrap(), first);
    }

    #[test]
    fn rejects_non_object_env_without_overwriting_the_file() {
        let dir = tempdir().expect("tempdir");
        let settings_path = dir.path().join("settings.json");
        fs::write(&settings_path, r#"{"env":"managed elsewhere"}"#).unwrap();
        let original = fs::read(&settings_path).unwrap();

        let error = configure_claude_settings_file(
            &settings_path,
            "127.0.0.1:8080".parse().unwrap(),
            "token",
        )
        .unwrap_err();

        assert!(error.to_string().contains("env must be a JSON object"));
        assert_eq!(fs::read(&settings_path).unwrap(), original);
    }

    #[cfg(unix)]
    #[test]
    fn preserves_a_user_settings_symlink() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("dotfiles/claude-settings.json");
        let settings_path = dir.path().join(".claude/settings.json");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(&target, r#"{"theme":"dark"}"#).unwrap();
        symlink(&target, &settings_path).unwrap();

        configure_claude_settings_file(&settings_path, "127.0.0.1:8080".parse().unwrap(), "token")
            .unwrap();

        assert!(
            fs::symlink_metadata(&settings_path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let settings: Value = serde_json::from_slice(&fs::read(&target).unwrap()).unwrap();
        assert_eq!(settings["theme"], "dark");
        assert_eq!(settings["env"]["OTEL_LOGS_EXPORTER"], "otlp");
    }
}

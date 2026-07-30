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
const CLAUDE_INTEGRATION_DIR: &str = "integrations/claude";
const CLAUDE_HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PermissionRequest",
    "Stop",
    "StopFailure",
    "SessionEnd",
];
const CLAUDE_INTEGRATION_FILES: &[(&str, &[u8], bool)] = &[
    ("package.json", br#"{"type":"module"}"#, false),
    (
        "src/context.js",
        include_bytes!("../../../clients/claude/src/context.js"),
        false,
    ),
    (
        "src/diagnostics.js",
        include_bytes!("../../../clients/claude/src/diagnostics.js"),
        false,
    ),
    (
        "src/discovery.js",
        include_bytes!("../../../clients/claude/src/discovery.js"),
        false,
    ),
    (
        "src/events.js",
        include_bytes!("../../../clients/claude/src/events.js"),
        false,
    ),
    (
        "src/hook.js",
        include_bytes!("../../../clients/claude/src/hook.js"),
        true,
    ),
    (
        "src/internal-api.js",
        include_bytes!("../../../clients/claude/src/internal-api.js"),
        false,
    ),
    (
        "src/reporter.js",
        include_bytes!("../../../clients/claude/src/reporter.js"),
        false,
    ),
    (
        "src/runtime-binding.js",
        include_bytes!("../../../clients/claude/src/runtime-binding.js"),
        false,
    ),
    (
        "src/workspace.js",
        include_bytes!("../../../clients/claude/src/workspace.js"),
        false,
    ),
];

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
    let home = PathBuf::from(home);
    let pontia_home = std::env::var_os("PONTIA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".pontia"));
    let settings_path = home.join(CLAUDE_SETTINGS_FILE);
    let integration_dir = pontia_home.join(CLAUDE_INTEGRATION_DIR);
    configure_claude_settings_file(&settings_path, &integration_dir, bind_addr, api_token)?;
    Ok(ClaudeApprovalIntegration::Configured { settings_path })
}

fn configure_claude_settings_file(
    settings_path: &Path,
    integration_dir: &Path,
    bind_addr: SocketAddr,
    api_token: &str,
) -> Result<()> {
    install_claude_integration(integration_dir)?;
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
        Value::String(otlp_logs_endpoint(bind_addr)),
    );
    env.insert(
        "OTEL_EXPORTER_OTLP_HEADERS".to_string(),
        Value::String(format!("Authorization=Bearer {api_token}")),
    );
    merge_claude_hooks(root, &integration_dir.join("src/hook.js"))?;

    write_settings_atomically(&write_path, &settings)
}

fn install_claude_integration(integration_dir: &Path) -> Result<()> {
    for (relative_path, contents, executable) in CLAUDE_INTEGRATION_FILES {
        let path = integration_dir.join(relative_path);
        let parent = path
            .parent()
            .ok_or_else(|| invalid_settings("Claude integration file has no parent directory"))?;
        fs::create_dir_all(parent)?;
        write_file_atomically(&path, contents, if *executable { 0o700 } else { 0o600 })?;
    }
    Ok(())
}

fn merge_claude_hooks(root: &mut Map<String, Value>, hook_path: &Path) -> Result<()> {
    let hooks = root
        .entry("hooks".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| invalid_settings("Claude settings hooks must be a JSON object"))?;
    let hook_command = hook_path.display().to_string();

    for event in CLAUDE_HOOK_EVENTS {
        let entries = hooks
            .entry((*event).to_string())
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or_else(|| {
                invalid_settings(format!(
                    "Claude settings hooks.{event} must be a JSON array"
                ))
            })?;
        entries.retain(|entry| !entry_uses_command(entry, &hook_command));
        entries.push(serde_json::json!({
            "hooks": [{
                "type": "command",
                "command": hook_command,
            }]
        }));
    }
    Ok(())
}

fn entry_uses_command(entry: &Value, command: &str) -> bool {
    entry
        .get("hooks")
        .and_then(Value::as_array)
        .is_some_and(|hooks| {
            hooks
                .iter()
                .any(|hook| hook.get("command").and_then(Value::as_str) == Some(command))
        })
}

fn otlp_logs_endpoint(bind_addr: SocketAddr) -> String {
    let endpoint_addr = if bind_addr.ip().is_unspecified() {
        SocketAddr::new(
            if bind_addr.is_ipv4() {
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
            } else {
                std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
            },
            bind_addr.port(),
        )
    } else {
        bind_addr
    };
    format!("http://{endpoint_addr}/internal/v1/otel/v1/logs")
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
    let mut contents = serde_json::to_vec_pretty(settings)?;
    contents.push(b'\n');
    write_file_atomically(settings_path, &contents, 0o600)
}

fn write_file_atomically(path: &Path, contents: &[u8], mode: u32) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid_settings(format!("{} has no parent directory", path.display())))?;
    fs::create_dir_all(parent)?;
    let (temp_path, mut file) = create_temp_file(parent)?;
    let result = (|| -> Result<()> {
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temp_path, path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn create_temp_file(parent: &Path) -> Result<(PathBuf, fs::File)> {
    for attempt in 0..100 {
        let path = parent.join(format!(
            ".pontia-write-{}-{attempt}.tmp",
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
        let integration_dir = dir.path().join(".pontia/integrations/claude");
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
            &integration_dir,
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
            settings["hooks"]["Notification"],
            json!([{"hooks": [{"type": "command", "command": "notify"}]}])
        );
        let hook_command = integration_dir.join("src/hook.js").display().to_string();
        for event in CLAUDE_HOOK_EVENTS {
            assert_eq!(
                settings["hooks"][event],
                json!([{"hooks": [{"type": "command", "command": hook_command}]}])
            );
        }
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
        let integration_dir = dir.path().join(".pontia/integrations/claude");
        let bind_addr = "127.0.0.1:8080".parse().unwrap();

        configure_claude_settings_file(&settings_path, &integration_dir, bind_addr, "token")
            .unwrap();
        let first = fs::read(&settings_path).unwrap();
        configure_claude_settings_file(&settings_path, &integration_dir, bind_addr, "token")
            .unwrap();

        assert_eq!(fs::read(&settings_path).unwrap(), first);
    }

    #[test]
    fn rejects_non_object_env_without_overwriting_the_file() {
        let dir = tempdir().expect("tempdir");
        let settings_path = dir.path().join("settings.json");
        let integration_dir = dir.path().join("integration");
        fs::write(&settings_path, r#"{"env":"managed elsewhere"}"#).unwrap();
        let original = fs::read(&settings_path).unwrap();

        let error = configure_claude_settings_file(
            &settings_path,
            &integration_dir,
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
        let integration_dir = dir.path().join(".pontia/integrations/claude");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::create_dir_all(settings_path.parent().unwrap()).unwrap();
        fs::write(&target, r#"{"theme":"dark"}"#).unwrap();
        symlink(&target, &settings_path).unwrap();

        configure_claude_settings_file(
            &settings_path,
            &integration_dir,
            "127.0.0.1:8080".parse().unwrap(),
            "token",
        )
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

    #[test]
    fn installs_the_embedded_hook_bundle_and_uses_reachable_receiver_addresses() {
        let dir = tempdir().expect("tempdir");
        let settings_path = dir.path().join(".claude/settings.json");
        let integration_dir = dir.path().join(".pontia/integrations/claude");

        configure_claude_settings_file(
            &settings_path,
            &integration_dir,
            "192.0.2.10:18080".parse().unwrap(),
            "token",
        )
        .unwrap();

        assert_eq!(
            fs::read(integration_dir.join("src/hook.js")).unwrap(),
            include_bytes!("../../../clients/claude/src/hook.js")
        );
        let settings: Value = serde_json::from_slice(&fs::read(&settings_path).unwrap()).unwrap();
        assert_eq!(
            settings["env"]["OTEL_EXPORTER_OTLP_LOGS_ENDPOINT"],
            "http://192.0.2.10:18080/internal/v1/otel/v1/logs"
        );
        assert_eq!(
            otlp_logs_endpoint("[::]:18081".parse().unwrap()),
            "http://[::1]:18081/internal/v1/otel/v1/logs"
        );
    }
}

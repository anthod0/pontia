use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::{
    error::{Error, Result},
    ids::new_runtime_instance_id,
};

pub(super) fn ensure_workspace_trusted(workspace: &Path) -> Result<()> {
    let config_path = claude_config_path()?;
    ensure_workspace_trusted_in_config(&config_path, workspace)
}

fn claude_config_path() -> Result<PathBuf> {
    // Test-only escape hatch: production must use Claude Code's real
    // ~/.claude.json, but tests need to isolate writes from the developer's
    // personal Claude config. Do not document this as a supported runtime knob.
    if let Some(path) = std::env::var_os("LLMPARTY_CLAUDE_CONFIG_PATH") {
        return Ok(PathBuf::from(path));
    }
    let home = home_dir().ok_or_else(|| {
        Error::Domain("cannot locate home directory for Claude Code trust config".to_string())
    })?;
    Ok(home.join(".claude.json"))
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
}

fn is_workspace_trusted(config: &Value, workspace_key: &str) -> bool {
    config
        .get("projects")
        .and_then(|projects| projects.get(workspace_key))
        .is_some_and(|project| {
            project.get("hasTrustDialogAccepted") == Some(&json!(true))
                && project.get("hasTrustDialogHooksAccepted") == Some(&json!(true))
        })
}

fn ensure_workspace_trusted_in_config(config_path: &Path, workspace: &Path) -> Result<()> {
    let workspace_key = workspace.display().to_string();
    let mut config = if config_path.exists() {
        let contents = std::fs::read_to_string(config_path)?;
        serde_json::from_str::<Value>(&contents).map_err(|err| {
            Error::Domain(format!(
                "failed to parse Claude Code config {}: {err}",
                config_path.display()
            ))
        })?
    } else {
        json!({})
    };

    if is_workspace_trusted(&config, &workspace_key) {
        return Ok(());
    }

    let config_object = config.as_object_mut().ok_or_else(|| {
        Error::Domain(format!(
            "Claude Code config {} must be a JSON object",
            config_path.display()
        ))
    })?;
    let projects = config_object
        .entry("projects".to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            Error::Domain(format!(
                "Claude Code config {} projects must be a JSON object",
                config_path.display()
            ))
        })?;
    let project = projects
        .entry(workspace_key)
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| {
            Error::Domain(format!(
                "Claude Code config {} project entry must be a JSON object",
                config_path.display()
            ))
        })?;

    project.insert("hasTrustDialogAccepted".to_string(), json!(true));
    project.insert("hasTrustDialogHooksAccepted".to_string(), json!(true));

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp_path = config_path.with_file_name(format!(
        "{}.{}.tmp",
        config_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(".claude.json"),
        new_runtime_instance_id()
    ));
    std::fs::write(&tmp_path, serde_json::to_string_pretty(&config)?)?;
    std::fs::rename(&tmp_path, config_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .expect("env lock")
    }

    #[test]
    fn trust_config_marks_workspace_and_hooks_accepted() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join(".claude.json");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");

        ensure_workspace_trusted_in_config(&config_path, &workspace).expect("trust config");

        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).expect("config contents"))
                .expect("json config");
        let project = &config["projects"][workspace.display().to_string()];
        assert_eq!(project["hasTrustDialogAccepted"], json!(true));
        assert_eq!(project["hasTrustDialogHooksAccepted"], json!(true));
    }

    #[test]
    fn config_path_uses_llmparty_override_when_set() {
        // LLMPARTY_CLAUDE_CONFIG_PATH is intentionally test-only; this verifies
        // tests can redirect config writes without touching ~/.claude.json.
        let _lock = env_lock();
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join("custom-claude.json");
        unsafe {
            std::env::set_var("LLMPARTY_CLAUDE_CONFIG_PATH", &config_path);
        }

        assert_eq!(claude_config_path().expect("config path"), config_path);

        unsafe {
            std::env::remove_var("LLMPARTY_CLAUDE_CONFIG_PATH");
        }
    }

    #[test]
    fn trust_config_does_not_rewrite_already_trusted_workspace() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join(".claude.json");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let original = json!({
            "projects": {
                workspace.display().to_string(): {
                    "hasTrustDialogAccepted": true,
                    "hasTrustDialogHooksAccepted": true
                }
            }
        })
        .to_string();
        std::fs::write(&config_path, &original).expect("write config");

        ensure_workspace_trusted_in_config(&config_path, &workspace).expect("trust config");

        assert_eq!(
            std::fs::read_to_string(&config_path).expect("config contents"),
            original
        );
    }

    #[test]
    fn trust_config_preserves_existing_project_state() {
        let dir = tempdir().expect("tempdir");
        let config_path = dir.path().join(".claude.json");
        let workspace = dir.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        std::fs::write(
            &config_path,
            json!({
                "theme": "dark",
                "projects": {
                    workspace.display().to_string(): {
                        "allowedTools": ["Read"],
                        "hasTrustDialogAccepted": false
                    }
                }
            })
            .to_string(),
        )
        .expect("write config");

        ensure_workspace_trusted_in_config(&config_path, &workspace).expect("trust config");

        let config: Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).expect("config contents"))
                .expect("json config");
        assert_eq!(config["theme"], json!("dark"));
        let project = &config["projects"][workspace.display().to_string()];
        assert_eq!(project["allowedTools"], json!(["Read"]));
        assert_eq!(project["hasTrustDialogAccepted"], json!(true));
        assert_eq!(project["hasTrustDialogHooksAccepted"], json!(true));
    }
}

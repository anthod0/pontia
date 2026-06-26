use std::path::PathBuf;

use pontia_core::error::{Error, Result};

use super::RuntimeStartRequest;

pub(super) fn workspace_path(request: &RuntimeStartRequest) -> Result<PathBuf> {
    let path = request
        .workspace
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::temp_dir()
                .join("pontia-workspaces")
                .join(&request.session_id)
        });
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

pub(super) struct LogPaths {
    pub(super) log_dir: PathBuf,
    pub(super) runtime_log: PathBuf,
    pub(super) pi_hook_log: PathBuf,
}

pub(super) fn log_paths(_session_id: &str) -> Result<LogPaths> {
    let log_dir = pontia_log_dir()?;
    Ok(LogPaths {
        runtime_log: log_dir.join("runtime.log"),
        pi_hook_log: log_dir.join("pi-hook.log"),
        log_dir,
    })
}

fn pontia_log_dir() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("PONTIA_HOME")
        && !path.trim().is_empty()
    {
        return Ok(PathBuf::from(path).join("state"));
    }
    let home = std::env::var("HOME").map_err(|_| Error::InvalidConfig {
        key: "HOME",
        message: "required to derive pontia home directory".to_string(),
    })?;
    Ok(PathBuf::from(home).join(".pontia/state"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_paths_default_to_pontia_home_state_dir() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        unsafe {
            std::env::remove_var("PONTIA_HOME");
            std::env::set_var("HOME", tempdir.path());
        }

        let paths = log_paths("sess_test").expect("log paths");

        assert_eq!(paths.log_dir, tempdir.path().join(".pontia/state"));
        assert_eq!(
            paths.runtime_log,
            tempdir.path().join(".pontia/state/runtime.log")
        );
        assert_eq!(
            paths.pi_hook_log,
            tempdir.path().join(".pontia/state/pi-hook.log")
        );
    }

    #[test]
    fn log_paths_respect_pontia_home() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        unsafe {
            std::env::set_var("PONTIA_HOME", tempdir.path());
        }

        let paths = log_paths("sess_test").expect("log paths");

        unsafe {
            std::env::remove_var("PONTIA_HOME");
        }
        assert_eq!(paths.log_dir, tempdir.path().join("state"));
        assert_eq!(paths.runtime_log, tempdir.path().join("state/runtime.log"));
        assert_eq!(paths.pi_hook_log, tempdir.path().join("state/pi-hook.log"));
    }
}

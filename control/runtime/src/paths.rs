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
    if let Ok(path) = std::env::var("PONTIA_LOG_DIR") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(path) = std::env::var("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("pontia"));
    }

    let home = std::env::var("HOME").map_err(|_| Error::InvalidConfig {
        key: "HOME",
        message: "required to derive pontia log directory".to_string(),
    })?;
    Ok(PathBuf::from(home).join(".local/state/pontia"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_paths_are_global_under_pontia_log_dir_not_session_runtime_dir() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        unsafe {
            std::env::set_var("PONTIA_LOG_DIR", tempdir.path());
        }

        let paths = log_paths("sess_test").expect("log paths");

        unsafe {
            std::env::remove_var("PONTIA_LOG_DIR");
        }
        assert_eq!(paths.log_dir, tempdir.path());
        assert_eq!(paths.runtime_log, tempdir.path().join("runtime.log"));
        assert_eq!(paths.pi_hook_log, tempdir.path().join("pi-hook.log"));
    }
}

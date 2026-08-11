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
}

impl LogPaths {
    pub(super) fn client_hook_log(&self, file_name: &str) -> PathBuf {
        self.log_dir.join(file_name)
    }
}

pub(super) fn log_paths(_session_id: &str) -> Result<LogPaths> {
    let log_dir = pontia_log_dir()?;
    Ok(LogPaths {
        runtime_log: log_dir.join("runtime.log"),
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
    use std::{
        ffi::OsString,
        sync::{Mutex, MutexGuard},
    };

    use super::*;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
        home: Option<OsString>,
        pontia_home: Option<OsString>,
    }

    impl EnvGuard {
        fn lock() -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
            Self {
                _lock: lock,
                home: std::env::var_os("HOME"),
                pontia_home: std::env::var_os("PONTIA_HOME"),
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            unsafe {
                restore_env("HOME", self.home.as_ref());
                restore_env("PONTIA_HOME", self.pontia_home.as_ref());
            }
        }
    }

    unsafe fn restore_env(key: &str, value: Option<&OsString>) {
        if let Some(value) = value {
            unsafe { std::env::set_var(key, value) };
        } else {
            unsafe { std::env::remove_var(key) };
        }
    }

    #[test]
    fn log_paths_default_to_pontia_home_state_dir() {
        let _env = EnvGuard::lock();
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
            paths.client_hook_log("pi-hook.log"),
            tempdir.path().join(".pontia/state/pi-hook.log")
        );
        assert_eq!(
            paths.client_hook_log("custom-hook.log"),
            tempdir.path().join(".pontia/state/custom-hook.log")
        );
    }

    #[test]
    fn log_paths_respect_pontia_home() {
        let _env = EnvGuard::lock();
        let tempdir = tempfile::tempdir().expect("tempdir");
        unsafe {
            std::env::set_var("PONTIA_HOME", tempdir.path());
        }

        let paths = log_paths("sess_test").expect("log paths");

        assert_eq!(paths.log_dir, tempdir.path().join("state"));
        assert_eq!(paths.runtime_log, tempdir.path().join("state/runtime.log"));
        assert_eq!(
            paths.client_hook_log("pi-hook.log"),
            tempdir.path().join("state/pi-hook.log")
        );
    }
}

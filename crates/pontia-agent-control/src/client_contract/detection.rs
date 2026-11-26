use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionStatus {
    Detected,
    NotDetected,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientDetection {
    pub status: DetectionStatus,
    pub detail: Option<String>,
}

impl ClientDetection {
    pub fn detected(detail: impl Into<String>) -> Self {
        Self {
            status: DetectionStatus::Detected,
            detail: Some(detail.into()),
        }
    }
    pub fn not_detected() -> Self {
        Self {
            status: DetectionStatus::NotDetected,
            detail: None,
        }
    }
    pub fn unknown(detail: impl Into<String>) -> Self {
        Self {
            status: DetectionStatus::Unknown,
            detail: Some(detail.into()),
        }
    }
}

/// The environment of the device performing detection, never the browser's host.
pub struct DetectionContext<'a> {
    pub vars: &'a HashMap<String, String>,
    pub pontia_home: &'a Path,
}

impl DetectionContext<'_> {
    pub fn search_path(&self) -> Vec<std::path::PathBuf> {
        pontia_runtime::executable::effective_search_path(
            self.vars.get("PATH").map(std::ffi::OsStr::new),
            self.vars.get("HOME").map(Path::new),
            Some(self.pontia_home),
        )
    }

    pub fn executable(&self, program: &str) -> Result<Option<std::path::PathBuf>, String> {
        self.executable_with_path(program, None)
    }

    pub fn executable_with_path(
        &self,
        program: &str,
        override_path: Option<&str>,
    ) -> Result<Option<std::path::PathBuf>, String> {
        match override_path {
            Some(path) => pontia_runtime::executable::find(program, Some(path)),
            None => {
                pontia_runtime::executable::find_matching(program, self.search_path(), |_| Ok(true))
            }
        }
    }

    pub async fn cli_version(&self, path: &Path, prefix: &str) -> Option<String> {
        super::version::probe(
            tokio::process::Command::new(path)
                .env_clear()
                .envs(self.vars),
            prefix,
        )
        .await
    }

    pub fn command(&self, program: &str) -> ClientDetection {
        self.command_with_path(program, None)
    }

    pub fn command_with_path(&self, program: &str, override_path: Option<&str>) -> ClientDetection {
        match self.executable_with_path(program, override_path) {
            Ok(Some(path)) => ClientDetection::detected(path.display().to_string()),
            Ok(None) => ClientDetection::not_detected(),
            Err(error) => ClientDetection::unknown(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};

    #[test]
    fn inherited_path_finds_new_managed_tools_while_an_override_stays_closed() {
        let root = tempfile::tempdir().unwrap();
        let inherited = root.path().join("inherited");
        let pontia = root.path().join("pontia");
        let vars = HashMap::from([
            ("PATH".into(), inherited.display().to_string()),
            ("HOME".into(), root.path().display().to_string()),
        ]);
        let context = DetectionContext {
            vars: &vars,
            pontia_home: &pontia,
        };
        assert!(context.executable("tool").unwrap().is_none());
        for directory in [
            root.path().join(".local/bin"),
            pontia.join("tools/bin"),
            inherited.clone(),
        ] {
            fs::create_dir_all(&directory).unwrap();
            let tool = directory.join("tool");
            fs::write(&tool, "").unwrap();
            fs::set_permissions(&tool, fs::Permissions::from_mode(0o700)).unwrap();
            assert_eq!(context.executable("tool").unwrap(), Some(tool));
            if directory != inherited {
                assert!(
                    context
                        .executable_with_path("tool", vars.get("PATH").map(String::as_str))
                        .unwrap()
                        .is_none()
                );
            }
        }
    }
}

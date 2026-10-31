use pontia_application::client_contract::{ClientIntegration, PreparedClientIntegration};
use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

pub fn integration() -> Arc<dyn ClientIntegration> {
    Arc::new(Integration)
}

pub(crate) fn node() -> std::ffi::OsString {
    std::env::var_os("PONTIA_CLAUDE_NODE_PATH").unwrap_or_else(|| "node".into())
}

struct Integration;
struct Prepared {
    node: PathBuf,
    host: Option<PathBuf>,
    root: PathBuf,
    config: PathBuf,
}

impl ClientIntegration for Integration {
    fn client_type(&self) -> &'static str {
        "claude-agent"
    }
    fn selected_by_default(&self) -> bool {
        false
    }
    fn skipped_summary(&self) -> Vec<String> {
        vec!["Claude Agent integration: skip".into()]
    }
    fn service_path_variables(&self) -> &'static [&'static str] {
        &[
            "PONTIA_CLAUDE_NODE_PATH",
            "PONTIA_CLAUDE_HOST_PATH",
            "CLAUDE_CONFIG_DIR",
        ]
    }
    fn prepare(
        &self,
        vars: &HashMap<String, String>,
        user_home: &Path,
        runner: &dyn CommandRunner,
    ) -> Result<Box<dyn PreparedClientIntegration>, String> {
        let configured = vars.get("PONTIA_CLAUDE_NODE_PATH");
        let program = configured.map(String::as_str).unwrap_or("node");
        // Resolve the same runtime into the service environment, including version-manager installs.
        let output = runner.run(program, &["-p".into(), "process.execPath".into()])?;
        let node = PathBuf::from(output.stdout.trim());
        if output.code != 0 || !node.is_absolute() {
            return Err("Install Node.js LTS and make node available on PATH".into());
        }
        let host = vars.get("PONTIA_CLAUDE_HOST_PATH").map(PathBuf::from);
        let root = vars
            .get("PONTIA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| user_home.join(".pontia"));
        let config = vars
            .get("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| user_home.join(".claude"));
        if host.as_ref().is_some_and(|path| !path.is_absolute())
            || !config.is_absolute()
            || !root.is_absolute()
        {
            return Err("Claude Agent host and configuration paths must be absolute".into());
        }
        Ok(Box::new(Prepared {
            node,
            host,
            root,
            config,
        }))
    }
}

impl PreparedClientIntegration for Prepared {
    fn summary(&self) -> Vec<String> {
        vec!["Claude Agent integration: enable".into()]
    }
    fn preflight(&self, runner: &dyn CommandRunner) -> Result<(), String> {
        let stage;
        let host = if let Some(host) = &self.host {
            host.clone()
        } else {
            stage = tempfile::tempdir().map_err(|e| e.to_string())?;
            crate::bundle::unpack(stage.path())?
        };
        if !host.is_file() {
            return Err("Claude Agent host is missing; reinstall Pontia".into());
        }
        let code = runner.run_status(
            self.node.to_str().ok_or("Node path is not UTF-8")?,
            &[
                host.to_str().ok_or("Host path is not UTF-8")?.into(),
                "preflight".into(),
            ],
        )?;
        if code != 0 {
            return Err(
                "Claude Agent dependency check failed; install Node.js LTS and reinstall Pontia"
                    .into(),
            );
        }
        Ok(())
    }
    fn install(&self, _: &dyn CommandRunner, _: &dyn DefinitionStore) -> Result<(), String> {
        if self.host.is_none() {
            crate::bundle::install(&self.root)?;
        }
        Ok(())
    }
    fn completion(&self) -> &'static str {
        "Configured Claude Agent"
    }
    fn service_environment_paths(&self) -> Vec<(String, PathBuf)> {
        let mut paths = vec![
            ("PONTIA_CLAUDE_NODE_PATH".into(), self.node.clone()),
            ("CLAUDE_CONFIG_DIR".into(), self.config.clone()),
        ];
        if let Some(host) = &self.host {
            paths.push(("PONTIA_CLAUDE_HOST_PATH".into(), host.clone()));
        }
        paths
    }
}

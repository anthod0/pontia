use pontia_agent_control::client_contract::{ClientIntegration, PreparedClientIntegration};
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

const HOST_SCRIPT: &str = "node_modules/@pontia/claude-client/dist/src/main.js";

pub(crate) fn host_path(root: &Path) -> PathBuf {
    root.join("clients/claude-client").join(HOST_SCRIPT)
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
        if let Some(host) = &self.host {
            return self.check_host(host, runner);
        }
        let host = host_path(&self.root);
        if host.is_file() {
            return self.check_host(&host, runner);
        }
        if runner.run_status("npm", &["--version".into()])? != 0 {
            return Err("Install Node.js LTS and npm before enabling Claude Agent".into());
        }
        Ok(())
    }
    fn install(&self, runner: &dyn CommandRunner, _: &dyn DefinitionStore) -> Result<(), String> {
        if self.host.is_some() || host_path(&self.root).is_file() {
            return Ok(());
        }
        let parent = self.root.join("clients");
        std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
        let stage = tempfile::tempdir_in(&parent).map_err(|e| e.to_string())?;
        let code = runner.run_interactive(
            "npm",
            &[
                "install".into(),
                "--prefix".into(),
                stage
                    .path()
                    .to_str()
                    .ok_or("Client path is not UTF-8")?
                    .into(),
                "--omit=dev".into(),
                "--no-audit".into(),
                "--no-fund".into(),
                "@pontia/claude-client".into(),
            ],
        )?;
        if code != 0 {
            return Err(format!(
                "Claude Agent client installation failed with exit code {code}"
            ));
        }
        self.check_host(&stage.path().join(HOST_SCRIPT), runner)?;
        std::fs::rename(stage.path(), parent.join("claude-client")).map_err(|e| e.to_string())?;
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

impl Prepared {
    fn check_host(&self, host: &Path, runner: &dyn CommandRunner) -> Result<(), String> {
        if !host.is_file() {
            return Err(
                "Claude Agent client is missing; run pontia init and select claude-agent".into(),
            );
        }
        let code = runner.run_status(
            self.node.to_str().ok_or("Node path is not UTF-8")?,
            &[
                host.to_str().ok_or("Host path is not UTF-8")?.into(),
                "preflight".into(),
            ],
        )?;
        if code != 0 {
            return Err("Claude Agent dependency check failed; check Node.js LTS and the @pontia/claude-client installation".into());
        }
        Ok(())
    }
}

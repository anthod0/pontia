use pontia_agent_control::client_contract::{
    ClientIntegration, PreparedClientIntegration, PreparedClientUninstall,
};
use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::{collections::HashMap, path::Path, sync::Arc};

mod uninstall;

pub fn integration() -> Arc<dyn ClientIntegration> {
    Arc::new(PiIntegration)
}
struct PiIntegration;

impl ClientIntegration for PiIntegration {
    fn maintain(
        &self,
        _vars: &HashMap<String, String>,
        _user_home: &Path,
        runner: &dyn CommandRunner,
        _definitions: &dyn DefinitionStore,
    ) -> Result<(), String> {
        self.preflight(runner)?;
        Self::install_plugin(runner, true)
    }

    fn prepare_uninstall(
        &self,
        vars: &HashMap<String, String>,
        user_home: &Path,
        runner: &dyn CommandRunner,
    ) -> Result<Option<Box<dyn PreparedClientUninstall>>, String> {
        uninstall::prepare(vars, user_home, runner)
    }
    fn client_type(&self) -> &'static str {
        "pi"
    }
    fn detect(
        &self,
        context: &pontia_agent_control::client_contract::DetectionContext<'_>,
    ) -> pontia_agent_control::client_contract::ClientDetection {
        context.command("pi")
    }
    fn skipped_summary(&self) -> Vec<String> {
        vec!["Skip pi".into()]
    }
    fn prepare(
        &self,
        _vars: &HashMap<String, String>,
        _user_home: &Path,
        _runner: &dyn CommandRunner,
    ) -> Result<Box<dyn PreparedClientIntegration>, String> {
        Ok(Box::new(PiIntegration))
    }
}

impl PreparedClientIntegration for PiIntegration {
    fn summary(&self) -> Vec<String> {
        vec!["Check and configure the pi integration".into()]
    }
    fn preflight(&self, runner: &dyn CommandRunner) -> Result<(), String> {
        let code = runner
            .run_status("pi", &["--version".into()])
            .map_err(|error| format!("pi must be installed and executable: {error}"))?;
        if code == 0 {
            Ok(())
        } else {
            Err(format!("pi --version failed with exit code {code}"))
        }
    }
    fn install(
        &self,
        runner: &dyn CommandRunner,
        _definitions: &dyn DefinitionStore,
    ) -> Result<(), String> {
        Self::install_plugin(runner, false)
    }
    fn completion(&self) -> &'static str {
        "Pi integration is ready"
    }
}

impl PiIntegration {
    fn install_plugin(runner: &dyn CommandRunner, update: bool) -> Result<(), String> {
        let packages =
            runner.run_with_env("pi", &["list".into()], &[("NO_COLOR".into(), "1".into())])?;
        if packages.code != 0 {
            return Err(format!("pi list failed with exit code {}", packages.code));
        }
        let mut source = "npm:@pontia/pi-client-plugin".to_string();
        let mut current: Option<String> = None;
        for line in packages.stdout.lines() {
            if line == "Project packages:" {
                break;
            }
            if let Some(path) = line.strip_prefix("    ") {
                let Some(candidate) = current.take() else {
                    continue;
                };
                let path = Path::new(path.trim());
                let manifest = match std::fs::read_to_string(path.join("package.json")) {
                    Ok(text) => serde_json::from_str::<serde_json::Value>(&text).ok(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.to_string()),
                };
                if let Some(manifest) =
                    manifest.filter(|manifest| manifest["name"] == "@pontia/pi-client-plugin")
                {
                    if (!update || !candidate.starts_with("npm:"))
                        && manifest["pi"]["extensions"]
                            .as_array()
                            .is_some_and(|extensions| {
                                !extensions.is_empty()
                                    && extensions.iter().all(|entry| {
                                        entry
                                            .as_str()
                                            .is_some_and(|entry| path.join(entry).is_file())
                                    })
                            })
                    {
                        return Ok(());
                    }
                    source = candidate;
                    if source.starts_with("npm:") {
                        if !update && let Some(version) = manifest["version"].as_str() {
                            source = format!("npm:@pontia/pi-client-plugin@{version}");
                        }
                    } else {
                        return Err(format!(
                            "Pi integration files are missing at {}; restore that package before retrying",
                            path.display()
                        ));
                    }
                }
            } else if let Some(candidate) = line.strip_prefix("  ") {
                let candidate = candidate.trim_end_matches(" (filtered)").to_string();
                if candidate == "npm:@pontia/pi-client-plugin"
                    || candidate.starts_with("npm:@pontia/pi-client-plugin@")
                {
                    source = candidate.clone();
                }
                current = Some(candidate);
            }
        }
        let code = runner
            .run_interactive("pi", &["install".into(), source])
            .map_err(|error| format!("failed to run pi install: {error}"))?;
        if code == 0 {
            Ok(())
        } else {
            Err(format!(
                "pi install npm:@pontia/pi-client-plugin failed with exit code {code}"
            ))
        }
    }
}

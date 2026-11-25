use pontia_agent_control::client_contract::{
    ClientIntegration, ClientOperation, ClientSetup, DetectionContext, PreparedClientIntegration,
    PreparedClientUninstall,
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
    fn detect_setup<'a>(
        &'a self,
        context: &'a DetectionContext<'_>,
    ) -> ClientOperation<'a, ClientSetup> {
        Box::pin(async move {
            let Some(pi) = context
                .executable("pi")
                .map_err(pontia_core::Error::Domain)?
            else {
                return Ok(ClientSetup::Unknown(
                    "Pi is unavailable to list user packages".into(),
                ));
            };
            let listing = pontia_agent_control::client_contract::version::bounded_output(
                tokio::process::Command::new(pi)
                    .arg("list")
                    .env_clear()
                    .envs(context.vars)
                    .env("NO_COLOR", "1")
                    .current_dir(context.pontia_home),
                256 * 1024,
            )
            .await
            .ok_or_else(|| pontia_core::Error::Domain("Cannot list Pi user packages".into()))?;
            match plugin_action(&listing, false).map_err(pontia_core::Error::Domain)? {
                PluginAction::Ready => Ok(ClientSetup::Initialized),
                PluginAction::Restore(path) => Ok(ClientSetup::Required(format!(
                    "Restore Pi plugin files at {path}"
                ))),
                _ => Ok(ClientSetup::Required(
                    "Install the Pontia Pi user plugin".into(),
                )),
            }
        })
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
        let args = match plugin_action(&packages.stdout, update)? {
            PluginAction::Ready => return Ok(()),
            PluginAction::Restore(path) => {
                return Err(format!(
                    "Pi integration files are missing at {path}; restore that package before retrying"
                ));
            }
            PluginAction::Install(source) => vec!["install".into(), source],
            // Pi otherwise also updates matching packages in a trusted project.
            PluginAction::Update(source) => vec!["update".into(), source, "--no-approve".into()],
        };
        let code = runner
            .run_interactive("pi", &args)
            .map_err(|error| format!("failed to run pi {}: {error}", args.join(" ")))?;
        if code == 0 {
            Ok(())
        } else {
            Err(format!(
                "pi {} failed with exit code {code}",
                args.join(" ")
            ))
        }
    }
}

enum PluginAction {
    Ready,
    Restore(String),
    Install(String),
    Update(String),
}

fn plugin_action(listing: &str, update: bool) -> Result<PluginAction, String> {
    let mut source = "npm:@pontia/pi-client-plugin".to_string();
    let mut update_source = None;
    let mut current: Option<String> = None;
    for line in listing.lines() {
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
                if manifest["pi"]["extensions"]
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
                    if update && candidate.starts_with("npm:") {
                        update_source = Some(candidate);
                        continue;
                    }
                    return Ok(PluginAction::Ready);
                }
                source = candidate;
                if source.starts_with("npm:") {
                    if !update && let Some(version) = manifest["version"].as_str() {
                        source = format!("npm:@pontia/pi-client-plugin@{version}");
                    }
                } else {
                    return Ok(PluginAction::Restore(path.display().to_string()));
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
    Ok(match update_source {
        Some(source) => PluginAction::Update(source),
        None => PluginAction::Install(source),
    })
}

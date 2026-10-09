use pontia_application::client_contract::{ClientIntegration, PreparedClientIntegration};
use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::{collections::HashMap, path::Path, sync::Arc};

pub fn integration() -> Arc<dyn ClientIntegration> {
    Arc::new(PiIntegration)
}
struct PiIntegration;

impl ClientIntegration for PiIntegration {
    fn client_type(&self) -> &'static str {
        "pi"
    }
    fn selected_by_default(&self) -> bool {
        true
    }
    fn skipped_summary(&self) -> Vec<String> {
        vec!["pi integration: skip".into()]
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
        vec!["pi integration: install".into()]
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
        let code = runner
            .run_interactive(
                "pi",
                &["install".into(), "npm:@pontia/pi-client-plugin".into()],
            )
            .map_err(|error| format!("failed to run pi install: {error}"))?;
        if code == 0 {
            Ok(())
        } else {
            Err(format!(
                "pi install npm:@pontia/pi-client-plugin failed with exit code {code}"
            ))
        }
    }
    fn completion(&self) -> &'static str {
        "Installed pi integration"
    }
}

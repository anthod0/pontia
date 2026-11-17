use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Client-owned local integration operations; the CLI owns selection and confirmation.
pub trait ClientIntegration: Send + Sync {
    fn client_type(&self) -> &'static str;
    fn setup_name(&self) -> &'static str {
        self.client_type()
    }
    fn detect(&self, context: &super::DetectionContext<'_>) -> super::ClientDetection;
    fn client_key(&self) -> &'static str {
        "default"
    }
    /// Maintain only integration artifacts, never upgrade the Agent Client itself.
    fn maintain(
        &self,
        vars: &HashMap<String, String>,
        user_home: &Path,
        runner: &dyn CommandRunner,
        definitions: &dyn DefinitionStore,
    ) -> Result<(), String> {
        let setup = self.prepare(vars, user_home, runner)?;
        setup.preflight(runner)?;
        setup.install(runner, definitions)
    }
    fn skipped_summary(&self) -> Vec<String>;
    fn service_path_variables(&self) -> &'static [&'static str] {
        &[]
    }
    fn prepare(
        &self,
        vars: &HashMap<String, String>,
        user_home: &Path,
        runner: &dyn CommandRunner,
    ) -> Result<Box<dyn PreparedClientIntegration>, String>;
    /// Inspect installed artifacts without requiring the client's setup dependencies.
    fn prepare_uninstall(
        &self,
        vars: &HashMap<String, String>,
        user_home: &Path,
        runner: &dyn CommandRunner,
    ) -> Result<Option<Box<dyn PreparedClientUninstall>>, String>;
}

pub trait PreparedClientUninstall {
    fn summary(&self) -> Vec<String>;
    fn uninstall(&self, runner: &dyn CommandRunner) -> Result<(), String>;
}

/// Preparation is read-only. Installation applies the recommended local configuration.
pub trait PreparedClientIntegration {
    fn summary(&self) -> Vec<String>;
    fn preflight(&self, runner: &dyn CommandRunner) -> Result<(), String>;
    fn install(
        &self,
        runner: &dyn CommandRunner,
        definitions: &dyn DefinitionStore,
    ) -> Result<(), String>;
    fn completion(&self) -> &'static str;
    fn next_steps(&self) -> Option<String> {
        None
    }
    fn service_environment_paths(&self) -> Vec<(String, PathBuf)> {
        Vec::new()
    }
}

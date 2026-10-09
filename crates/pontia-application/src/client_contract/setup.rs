use pontia_runtime::local_service::{CommandRunner, DefinitionStore};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// Client-owned local integration operations; the CLI owns selection and confirmation.
pub trait ClientIntegration: Send + Sync {
    fn client_type(&self) -> &'static str;
    fn selected_by_default(&self) -> bool;
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
}

/// Preparation is read-only. Installation runs only after the CLI's confirmation.
pub trait PreparedClientIntegration {
    fn summary(&self) -> Vec<String>;
    fn preflight(&self, runner: &dyn CommandRunner) -> Result<(), String>;
    fn install(
        &self,
        runner: &dyn CommandRunner,
        definitions: &dyn DefinitionStore,
    ) -> Result<(), String>;
    fn completion(&self) -> &'static str;
    fn service_environment_paths(&self) -> Vec<(String, PathBuf)> {
        Vec::new()
    }
}

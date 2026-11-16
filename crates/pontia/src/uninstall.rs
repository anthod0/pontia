use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use pontia_agent_control::client_contract::{ClientIntegration, PreparedClientUninstall};
use pontia_runtime::local_service::{CommandRunner, DefinitionStore, absolute_utf8_path};

use crate::{lifecycle::ServiceManager, runtime_io::FileDefinitionStore};

pub struct PreparedUninstall {
    home: PathBuf,
    service: Option<PathBuf>,
    clients: Vec<Box<dyn PreparedClientUninstall>>,
    pontia_home: PathBuf,
}

impl PreparedUninstall {
    pub fn prepare(
        home: &Path,
        vars: &HashMap<String, String>,
        manager: &impl ServiceManager,
        runner: &dyn CommandRunner,
        integrations: &[Arc<dyn ClientIntegration>],
    ) -> Result<Self, String> {
        let path = manager.definition_path(home);
        let definition = FileDefinitionStore.read(&path)?;
        let mut vars = vars.clone();
        if let Some(definition) = &definition {
            vars.insert(
                "PONTIA_HOME".into(),
                manager.persisted_home(definition)?.display().to_string(),
            );
            for (name, path) in manager.persisted_client_paths(definition)? {
                vars.insert(name, path.display().to_string());
            }
        }
        let root = vars
            .get("PONTIA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".pontia"));
        absolute_utf8_path(&root, "PONTIA_HOME")?;
        vars.insert("PONTIA_HOME".into(), root.display().to_string());
        let mut clients = Vec::new();
        for integration in integrations {
            if let Some(client) = integration.prepare_uninstall(&vars, home, runner)? {
                clients.push(client);
            }
        }
        Ok(Self {
            home: home.into(),
            service: definition.map(|_| path),
            clients,
            pontia_home: root,
        })
    }

    pub fn service_installed(&self) -> bool {
        self.service.is_some()
    }

    pub fn remaining_data_hint(&self) -> String {
        let path = self
            .pontia_home
            .display()
            .to_string()
            .replace('\'', "'\\''");
        format!(
            "To permanently delete the remaining Pontia data, including configuration, credentials, and history, run:\n  rm -rf -- '{path}'"
        )
    }

    pub fn service_config(&self) -> Result<pontia_config::AppConfig, String> {
        pontia_clients::config_from_vars(&HashMap::from([(
            "PONTIA_HOME".into(),
            self.pontia_home.display().to_string(),
        )]))
        .map_err(|error| error.to_string())
    }

    pub fn summary(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(path) = &self.service {
            lines.push(format!(
                "Stop Pontia and remove its service: {}",
                path.display()
            ));
        }
        for client in &self.clients {
            lines.extend(client.summary());
        }
        lines
    }

    pub fn uninstall(
        self,
        manager: &impl ServiceManager,
        runner: &dyn CommandRunner,
    ) -> Result<(), String> {
        if self.service.is_some() {
            manager.down()?;
        }
        for client in self.clients {
            client.uninstall(runner)?;
        }
        // Retain the installed environment until all client cleanup succeeds, so a retry
        // can still locate integrations when the invoking shell uses another PONTIA_HOME.
        if self.service.is_some() {
            manager.uninstall(&self.home)?;
        }
        Ok(())
    }
}

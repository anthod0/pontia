use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
    time::Duration,
};

use pontia_config::AppConfig;

const READINESS_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnabledState {
    Enabled,
    Disabled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Running,
    Stopped,
    Starting,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceStatus {
    pub enabled: EnabledState,
    pub loaded: bool,
    pub run_state: RunState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UpOptions {
    pub restart_running: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleStatus {
    pub definition_installed: bool,
    pub service: ServiceStatus,
    pub http_healthy: bool,
    pub persisted_home: Option<PathBuf>,
}

impl LifecycleStatus {
    pub fn is_operational(&self) -> bool {
        self.service.run_state == RunState::Running && self.http_healthy
    }
}

pub trait DefinitionStore {
    fn read(&self, path: &Path) -> Result<Option<String>, String>;
    fn install(&self, path: &Path, contents: &str) -> Result<bool, String>;
}

pub trait ServiceManager {
    fn definition_path(&self, user_home: &Path) -> PathBuf;
    fn render_definition(&self, pontiad: &Path, pontia_home: &Path) -> Result<String, String>;
    fn persisted_home(&self, definition: &str) -> Result<PathBuf, String>;
    fn status(&self) -> Result<ServiceStatus, String>;
    fn failure_diagnostic(&self) -> Result<String, String>;
    fn apply(
        &self,
        definition_path: &Path,
        definition_changed: bool,
        restart_running: bool,
        previous: ServiceStatus,
    ) -> Result<(), String>;
    fn down(&self) -> Result<(), String>;
}

pub trait HealthProbe {
    fn wait_until_healthy(
        &self,
        addr: SocketAddr,
        timeout: Duration,
        keep_waiting: &mut dyn FnMut() -> Result<bool, String>,
    ) -> Result<bool, String>;
    fn is_healthy(&self, addr: SocketAddr) -> Result<bool, String>;
}

pub struct Lifecycle<'a, M, S, H> {
    manager: &'a M,
    definitions: &'a S,
    health: &'a H,
}

impl<'a, M, S, H> Lifecycle<'a, M, S, H>
where
    M: ServiceManager,
    S: DefinitionStore,
    H: HealthProbe,
{
    pub fn new(manager: &'a M, definitions: &'a S, health: &'a H) -> Self {
        Self {
            manager,
            definitions,
            health,
        }
    }

    pub fn up(
        &self,
        config: &AppConfig,
        pontiad: &Path,
        user_home: &Path,
        options: UpOptions,
    ) -> Result<(), String> {
        let previous = self.manager.status()?;
        let path = self.manager.definition_path(user_home);
        let rendered = self
            .manager
            .render_definition(pontiad, &config.pontia_home)?;
        let definition_changed = self.definitions.install(&path, &rendered)?;
        let restart_running = previous.run_state == RunState::Running
            && (definition_changed || options.restart_running);
        self.manager
            .apply(&path, definition_changed, restart_running, previous)?;

        let addr = local_health_addr(config.bind_addr);
        let mut service_failed = false;
        let healthy = self
            .health
            .wait_until_healthy(addr, READINESS_TIMEOUT, &mut || {
                service_failed = self.manager.status()?.run_state == RunState::Failed;
                Ok(!service_failed)
            })?;
        if !healthy {
            if service_failed {
                return Err(format!(
                    "pontiad failed to start: {}",
                    self.manager.failure_diagnostic()?
                ));
            }
            return Err(format!(
                "pontiad did not become healthy at http://{addr}/healthz within {} seconds",
                READINESS_TIMEOUT.as_secs()
            ));
        }
        Ok(())
    }

    pub fn down(&self) -> Result<(), String> {
        self.manager.down()
    }

    pub fn status(&self, user_home: &Path) -> Result<LifecycleStatus, String> {
        let service = self.manager.status()?;
        let definition = self
            .definitions
            .read(&self.manager.definition_path(user_home))?;
        let persisted_home = definition
            .as_deref()
            .map(|definition| self.manager.persisted_home(definition))
            .transpose()?;
        let http_healthy = match persisted_home.as_ref() {
            Some(home) if service.run_state == RunState::Running => {
                let config = AppConfig::from_vars(&HashMap::from([(
                    "PONTIA_HOME".to_string(),
                    home.display().to_string(),
                )]))
                .map_err(|error| error.to_string())?;
                self.health
                    .is_healthy(local_health_addr(config.bind_addr))?
            }
            _ => false,
        };

        Ok(LifecycleStatus {
            definition_installed: definition.is_some(),
            service,
            http_healthy,
            persisted_home,
        })
    }
}

fn local_health_addr(addr: SocketAddr) -> SocketAddr {
    let ip = if addr.ip().is_unspecified() {
        match addr.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        }
    } else {
        addr.ip()
    };
    SocketAddr::new(ip, addr.port())
}

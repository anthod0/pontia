use std::{
    cell::RefCell,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use pontia::lifecycle::{
    DefinitionStore, EnabledState, HealthProbe, Lifecycle, RunState, ServiceManager, ServiceStatus,
    UpOptions,
};
use pontia_config::AppConfig;

#[derive(Default)]
struct FakeStore {
    installed: RefCell<Vec<(PathBuf, String)>>,
    existing: Option<String>,
    changed: bool,
}

impl DefinitionStore for FakeStore {
    fn read(&self, _path: &Path) -> Result<Option<String>, String> {
        Ok(self.existing.clone())
    }

    fn install(&self, path: &Path, contents: &str) -> Result<bool, String> {
        self.installed
            .borrow_mut()
            .push((path.to_path_buf(), contents.to_string()));
        Ok(self.changed)
    }
}

struct FakeManager {
    status: ServiceStatus,
    apply_calls: RefCell<Vec<(PathBuf, bool, bool, ServiceStatus)>>,
    down_calls: RefCell<usize>,
}

impl ServiceManager for FakeManager {
    fn definition_path(&self, user_home: &Path) -> PathBuf {
        user_home.join("service-definition")
    }

    fn render_definition(&self, pontiad: &Path, pontia_home: &Path) -> Result<String, String> {
        Ok(format!("{}|{}", pontiad.display(), pontia_home.display()))
    }

    fn persisted_home(&self, definition: &str) -> Result<PathBuf, String> {
        definition
            .split_once('|')
            .map(|(_, home)| PathBuf::from(home))
            .ok_or_else(|| "malformed definition".to_string())
    }

    fn status(&self) -> Result<ServiceStatus, String> {
        Ok(self.status)
    }

    fn apply(
        &self,
        definition_path: &Path,
        definition_changed: bool,
        restart_running: bool,
        previous: ServiceStatus,
    ) -> Result<(), String> {
        self.apply_calls.borrow_mut().push((
            definition_path.to_path_buf(),
            definition_changed,
            restart_running,
            previous,
        ));
        Ok(())
    }

    fn down(&self) -> Result<(), String> {
        *self.down_calls.borrow_mut() += 1;
        Ok(())
    }
}

struct FakeHealth {
    healthy: bool,
    waits: RefCell<Vec<(SocketAddr, Duration)>>,
    probes: RefCell<Vec<SocketAddr>>,
}

impl HealthProbe for FakeHealth {
    fn wait_until_healthy(&self, addr: SocketAddr, timeout: Duration) -> Result<bool, String> {
        self.waits.borrow_mut().push((addr, timeout));
        Ok(self.healthy)
    }

    fn is_healthy(&self, addr: SocketAddr) -> Result<bool, String> {
        self.probes.borrow_mut().push(addr);
        Ok(self.healthy)
    }
}

fn config(home: &Path) -> AppConfig {
    AppConfig::from_vars(&std::collections::HashMap::from([(
        "PONTIA_HOME".to_string(),
        home.display().to_string(),
    )]))
    .expect("valid config")
}

fn manager(status: ServiceStatus) -> FakeManager {
    FakeManager {
        status,
        apply_calls: RefCell::new(Vec::new()),
        down_calls: RefCell::new(0),
    }
}

#[test]
fn up_renders_installs_applies_and_waits_for_health_without_real_io() {
    let store = FakeStore {
        changed: true,
        ..FakeStore::default()
    };
    let previous = ServiceStatus {
        enabled: EnabledState::Enabled,
        loaded: true,
        run_state: RunState::Running,
    };
    let manager = manager(previous);
    let health = FakeHealth {
        healthy: true,
        waits: RefCell::new(Vec::new()),
        probes: RefCell::new(Vec::new()),
    };
    let lifecycle = Lifecycle::new(&manager, &store, &health);
    let pontia_home = Path::new("/home/alice/.pontia");

    lifecycle
        .up(
            &config(pontia_home),
            Path::new("/opt/pontia/bin/pontiad"),
            Path::new("/home/alice"),
            UpOptions::default(),
        )
        .expect("up succeeds");

    assert_eq!(
        store.installed.into_inner(),
        vec![(
            PathBuf::from("/home/alice/service-definition"),
            "/opt/pontia/bin/pontiad|/home/alice/.pontia".to_string(),
        )]
    );
    assert_eq!(
        manager.apply_calls.into_inner(),
        vec![(
            PathBuf::from("/home/alice/service-definition"),
            true,
            true,
            previous,
        )]
    );
    assert_eq!(
        health.waits.into_inner(),
        vec![("127.0.0.1:8080".parse().unwrap(), Duration::from_secs(15))]
    );
}

#[test]
fn up_restarts_a_running_service_when_restart_is_requested() {
    let store = FakeStore::default();
    let previous = ServiceStatus {
        enabled: EnabledState::Enabled,
        loaded: true,
        run_state: RunState::Running,
    };
    let manager = manager(previous);
    let health = FakeHealth {
        healthy: true,
        waits: RefCell::new(Vec::new()),
        probes: RefCell::new(Vec::new()),
    };
    let lifecycle = Lifecycle::new(&manager, &store, &health);

    lifecycle
        .up(
            &config(Path::new("/home/alice/.pontia")),
            Path::new("/opt/pontiad"),
            Path::new("/home/alice"),
            UpOptions {
                restart_running: true,
            },
        )
        .expect("up succeeds");

    assert_eq!(
        manager.apply_calls.into_inner(),
        vec![(
            PathBuf::from("/home/alice/service-definition"),
            false,
            true,
            previous,
        )]
    );
}

#[test]
fn up_starts_without_requesting_a_restart_when_config_changed_while_stopped() {
    let store = FakeStore::default();
    let previous = ServiceStatus {
        enabled: EnabledState::Disabled,
        loaded: false,
        run_state: RunState::Stopped,
    };
    let manager = manager(previous);
    let health = FakeHealth {
        healthy: true,
        waits: RefCell::new(Vec::new()),
        probes: RefCell::new(Vec::new()),
    };
    let lifecycle = Lifecycle::new(&manager, &store, &health);

    lifecycle
        .up(
            &config(Path::new("/home/alice/.pontia")),
            Path::new("/opt/pontiad"),
            Path::new("/home/alice"),
            UpOptions {
                restart_running: true,
            },
        )
        .expect("up succeeds");

    assert_eq!(
        manager.apply_calls.into_inner(),
        vec![(
            PathBuf::from("/home/alice/service-definition"),
            false,
            false,
            previous,
        )]
    );
}

#[test]
fn up_does_not_restart_a_running_service_when_nothing_changed() {
    let store = FakeStore::default();
    let previous = ServiceStatus {
        enabled: EnabledState::Enabled,
        loaded: true,
        run_state: RunState::Running,
    };
    let manager = manager(previous);
    let health = FakeHealth {
        healthy: true,
        waits: RefCell::new(Vec::new()),
        probes: RefCell::new(Vec::new()),
    };
    let lifecycle = Lifecycle::new(&manager, &store, &health);

    lifecycle
        .up(
            &config(Path::new("/home/alice/.pontia")),
            Path::new("/opt/pontiad"),
            Path::new("/home/alice"),
            UpOptions::default(),
        )
        .expect("up succeeds");

    assert_eq!(
        manager.apply_calls.into_inner(),
        vec![(
            PathBuf::from("/home/alice/service-definition"),
            false,
            false,
            previous,
        )]
    );
}

#[test]
fn up_fails_when_health_does_not_become_ready() {
    let store = FakeStore::default();
    let manager = manager(ServiceStatus {
        enabled: EnabledState::Disabled,
        loaded: false,
        run_state: RunState::Stopped,
    });
    let health = FakeHealth {
        healthy: false,
        waits: RefCell::new(Vec::new()),
        probes: RefCell::new(Vec::new()),
    };
    let lifecycle = Lifecycle::new(&manager, &store, &health);

    let error = lifecycle
        .up(
            &config(Path::new("/home/alice/.pontia")),
            Path::new("/opt/pontiad"),
            Path::new("/home/alice"),
            UpOptions::default(),
        )
        .expect_err("unhealthy daemon must fail");

    assert!(error.contains("did not become healthy"), "{error}");
}

#[test]
fn status_uses_persisted_home_and_requires_running_healthy_daemon() {
    let store = FakeStore {
        existing: Some("/opt/pontiad|/srv/pontia".to_string()),
        ..FakeStore::default()
    };
    let manager = manager(ServiceStatus {
        enabled: EnabledState::Enabled,
        loaded: true,
        run_state: RunState::Running,
    });
    let health = FakeHealth {
        healthy: true,
        waits: RefCell::new(Vec::new()),
        probes: RefCell::new(Vec::new()),
    };
    let lifecycle = Lifecycle::new(&manager, &store, &health);

    let status = lifecycle
        .status(Path::new("/home/alice"))
        .expect("status succeeds");

    assert!(status.definition_installed);
    assert_eq!(status.persisted_home, Some(PathBuf::from("/srv/pontia")));
    assert!(status.http_healthy);
    assert!(status.is_operational());
    assert_eq!(
        health.probes.into_inner(),
        vec!["127.0.0.1:8080".parse().unwrap()]
    );
}

#[test]
fn missing_definition_is_reported_without_health_probe() {
    let store = FakeStore::default();
    let manager = manager(ServiceStatus {
        enabled: EnabledState::Disabled,
        loaded: false,
        run_state: RunState::Stopped,
    });
    let health = FakeHealth {
        healthy: true,
        waits: RefCell::new(Vec::new()),
        probes: RefCell::new(Vec::new()),
    };
    let lifecycle = Lifecycle::new(&manager, &store, &health);

    let status = lifecycle
        .status(Path::new("/home/alice"))
        .expect("status succeeds");

    assert!(!status.definition_installed);
    assert_eq!(status.persisted_home, None);
    assert!(!status.http_healthy);
    assert!(!status.is_operational());
    assert!(health.probes.into_inner().is_empty());
}

#[test]
fn down_delegates_to_fixed_service_identity() {
    let store = FakeStore::default();
    let manager = manager(ServiceStatus {
        enabled: EnabledState::Disabled,
        loaded: false,
        run_state: RunState::Stopped,
    });
    let health = FakeHealth {
        healthy: false,
        waits: RefCell::new(Vec::new()),
        probes: RefCell::new(Vec::new()),
    };
    let lifecycle = Lifecycle::new(&manager, &store, &health);

    lifecycle.down().expect("down succeeds");

    assert_eq!(manager.down_calls.into_inner(), 1);
}

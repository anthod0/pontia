use std::{future::Future, path::Path, time::Duration};

use tokio::process::Command;

use anyhow::{Context, Result};

use crate::files::atomic_write;

pub const UNIT_PATH: &str = "/etc/systemd/system/pontia-edge.service";
pub const UNIT_DIRECTORY: &str = "/etc/systemd/system";
const UPDATE_SERVICE: &str = "pontia-edge-update.service";
const UPDATE_TIMER: &str = "pontia-edge-update.timer";
const UPDATE_SERVICE_CONTENTS: &str = include_str!("../systemd/pontia-edge-update.service");
const UPDATE_TIMER_CONTENTS: &str = include_str!("../systemd/pontia-edge-update.timer");
pub const UNIT: &str = r#"[Unit]
Description=Pontia Edge
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/pontia-edge
Restart=on-failure
RestartSec=5s
User=root
NoNewPrivileges=true
PrivateTmp=true
ProtectHome=true
ProtectSystem=strict
ReadWritePaths=/etc/pontia/edge

[Install]
WantedBy=multi-user.target
"#;

pub trait CommandRunner {
    fn run(&self, arguments: &[&str]) -> impl Future<Output = Result<bool>> + Send;
}

pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    async fn run(&self, arguments: &[&str]) -> Result<bool> {
        let mut command = Command::new("systemctl");
        command.args(arguments).kill_on_drop(true);
        Ok(
            tokio::time::timeout(Duration::from_secs(30), command.status())
                .await
                .with_context(|| {
                    format!(
                        "systemctl {} timed out after 30 seconds",
                        arguments.join(" ")
                    )
                })?
                .with_context(|| format!("failed to run systemctl {}", arguments.join(" ")))?
                .success(),
        )
    }
}

pub struct Systemd<R = ProcessCommandRunner> {
    runner: R,
}

impl Default for Systemd {
    fn default() -> Self {
        Self {
            runner: ProcessCommandRunner,
        }
    }
}

impl<R: CommandRunner> Systemd<R> {
    pub async fn is_active(&self) -> bool {
        self.runner
            .run(&["is-active", "--quiet", "pontia-edge.service"])
            .await
            .unwrap_or(false)
    }

    pub async fn update_is_active(&self) -> Result<bool> {
        self.runner
            .run(&["is-active", "--quiet", "pontia-edge.service"])
            .await
    }

    pub async fn restart(&self) -> Result<()> {
        self.run_required(&["restart", "pontia-edge.service"]).await
    }

    pub async fn install_and_start(&self, unit_path: &Path) -> Result<()> {
        atomic_write(unit_path, UNIT.as_bytes(), 0o644)?;
        self.run_required(&["daemon-reload"]).await?;
        self.run_required(&["enable", "--now", "pontia-edge.service"])
            .await
    }

    pub async fn enable_auto_update(&self, unit_directory: &Path) -> Result<()> {
        atomic_write(
            &unit_directory.join(UPDATE_SERVICE),
            UPDATE_SERVICE_CONTENTS.as_bytes(),
            0o644,
        )?;
        atomic_write(
            &unit_directory.join(UPDATE_TIMER),
            UPDATE_TIMER_CONTENTS.as_bytes(),
            0o644,
        )?;
        self.run_required(&["daemon-reload"]).await?;
        self.run_required(&["enable", "--now", UPDATE_TIMER]).await
    }

    pub async fn disable_auto_update(&self) -> Result<()> {
        // Do not stop an update already in progress: it may be rolling back.
        self.run_required(&["disable", "--now", UPDATE_TIMER]).await
    }

    pub async fn show_auto_update(&self) -> Result<()> {
        self.run_required(&[
            "show",
            UPDATE_TIMER,
            "--property=LoadState,UnitFileState,ActiveState,NextElapseUSecRealtime,LastTriggerUSec",
        ])
        .await?;
        self.run_required(&[
            "show",
            UPDATE_SERVICE,
            "--property=ActiveState,Result,ExecMainStatus,ExecMainExitTimestamp",
        ])
        .await
    }

    async fn run_required(&self, arguments: &[&str]) -> Result<()> {
        anyhow::ensure!(
            self.runner.run(arguments).await?,
            "systemctl {} failed",
            arguments.join(" ")
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct FakeRunner {
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl CommandRunner for FakeRunner {
        async fn run(&self, arguments: &[&str]) -> Result<bool> {
            self.calls
                .lock()
                .unwrap()
                .push(arguments.iter().map(|value| (*value).to_owned()).collect());
            Ok(true)
        }
    }

    struct TimerRunner {
        directory: std::path::PathBuf,
        state: Mutex<TimerState>,
    }

    #[derive(Default)]
    struct TimerState {
        loaded: bool,
        enabled: bool,
        active: bool,
        update_running: bool,
    }

    impl CommandRunner for TimerRunner {
        async fn run(&self, arguments: &[&str]) -> Result<bool> {
            let mut state = self.state.lock().unwrap();
            match arguments {
                ["daemon-reload"] => {
                    state.loaded = self.directory.join(UPDATE_SERVICE).is_file()
                        && self.directory.join(UPDATE_TIMER).is_file();
                    Ok(state.loaded)
                }
                ["enable", "--now", UPDATE_TIMER] if state.loaded => {
                    state.enabled = true;
                    state.active = true;
                    Ok(true)
                }
                ["disable", "--now", UPDATE_TIMER] => {
                    state.enabled = false;
                    state.active = false;
                    Ok(true)
                }
                ["stop", UPDATE_SERVICE] => {
                    state.update_running = false;
                    Ok(true)
                }
                _ => anyhow::bail!("unexpected systemctl arguments: {arguments:?}"),
            }
        }
    }

    #[tokio::test]
    async fn enabling_installs_and_activates_timer_and_disabling_preserves_in_flight_update() {
        let root = tempfile::tempdir().unwrap();
        let systemd = Systemd {
            runner: TimerRunner {
                directory: root.path().to_path_buf(),
                state: Mutex::new(TimerState {
                    update_running: true,
                    ..Default::default()
                }),
            },
        };
        for _ in 0..2 {
            systemd.enable_auto_update(root.path()).await.unwrap();
            let state = systemd.runner.state.lock().unwrap();
            assert!(state.enabled && state.active);
        }
        let service = std::fs::read_to_string(root.path().join(UPDATE_SERVICE)).unwrap();
        let timer = std::fs::read_to_string(root.path().join(UPDATE_TIMER)).unwrap();
        assert!(service.contains("ExecStart=/usr/local/bin/pontia-edge update --automatic"));
        assert!(service.contains("ReadWritePaths=/usr/local/bin"));
        assert!(timer.contains("OnCalendar=hourly"));
        assert!(timer.contains("RandomizedDelaySec=10m"));
        assert!(timer.contains("Persistent=true"));
        systemd.disable_auto_update().await.unwrap();
        let state = systemd.runner.state.lock().unwrap();
        assert!(!state.enabled && !state.active);
        assert!(state.update_running);
    }

    #[tokio::test]
    async fn cannot_enable_updates_when_unit_installation_fails() {
        let root = tempfile::tempdir().unwrap();
        let not_directory = root.path().join("file");
        std::fs::write(&not_directory, b"occupied").unwrap();
        let systemd = Systemd {
            runner: TimerRunner {
                directory: not_directory.clone(),
                state: Mutex::new(TimerState::default()),
            },
        };
        assert!(systemd.enable_auto_update(&not_directory).await.is_err());
        let state = systemd.runner.state.lock().unwrap();
        assert!(!state.enabled && !state.active);
    }

    #[tokio::test]
    async fn update_checks_status_and_restarts_without_reinstalling_the_unit() {
        let systemd = Systemd {
            runner: FakeRunner {
                calls: Mutex::new(Vec::new()),
            },
        };
        assert!(systemd.update_is_active().await.unwrap());
        systemd.restart().await.unwrap();
        assert_eq!(
            *systemd.runner.calls.lock().unwrap(),
            [
                vec![
                    "is-active".to_owned(),
                    "--quiet".to_owned(),
                    "pontia-edge.service".to_owned()
                ],
                vec!["restart".to_owned(), "pontia-edge.service".to_owned()],
            ]
        );
    }

    #[tokio::test]
    async fn restart_failure_is_not_reported_as_success() {
        struct FailingRunner;
        impl CommandRunner for FailingRunner {
            async fn run(&self, _arguments: &[&str]) -> Result<bool> {
                Ok(false)
            }
        }
        let systemd = Systemd {
            runner: FailingRunner,
        };
        assert!(!systemd.update_is_active().await.unwrap());
        assert!(systemd.restart().await.is_err());
    }

    #[tokio::test]
    async fn installs_the_embedded_unit_before_reloading_and_starting_systemd() {
        let test_root = tempfile::tempdir().unwrap();
        let unit_path = test_root.path().join("pontia-edge.service");
        let systemd = Systemd {
            runner: FakeRunner {
                calls: Mutex::new(Vec::new()),
            },
        };

        systemd.install_and_start(&unit_path).await.unwrap();

        assert_eq!(std::fs::read_to_string(unit_path).unwrap(), UNIT);
        assert_eq!(
            *systemd.runner.calls.lock().unwrap(),
            [
                vec!["daemon-reload".to_owned()],
                vec![
                    "enable".to_owned(),
                    "--now".to_owned(),
                    "pontia-edge.service".to_owned()
                ],
            ]
        );
    }
}

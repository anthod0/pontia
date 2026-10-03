use std::{future::Future, path::Path, time::Duration};

use tokio::process::Command;

use anyhow::{Context, Result};

use crate::files::atomic_write;

pub const UNIT_PATH: &str = "/etc/systemd/system/pontia-edge.service";
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
